// Watches for other apps capturing the microphone and prompts the user to
// start recording. Entirely self-contained — removing it only requires
// deleting `mod automation;` from lib.rs.
//
// Previously this spawned a Swift sidecar (mic-monitor) and parsed its
// stdout. This version calls the same CoreAudio C APIs directly from Rust,
// eliminating the subprocess and the respawn loop.
//
// macOS 14+: per-process detection via kAudioHardwarePropertyProcessObjectList
// macOS 12–13: device-level fallback via kAudioDevicePropertyDeviceIsRunningSomewhere

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::{HashMap, HashSet};
    use std::os::raw::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use chrono::Local;
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    use tauri::{AppHandle, Emitter, Manager, Runtime};

    // ---------------------------------------------------------------------------
    // CoreAudio FFI
    // ---------------------------------------------------------------------------

    type OSStatus = i32;
    type AudioObjectID = u32;

    const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676c_6f62; // 'glob'
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
    const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_INPUT_DEVICE: u32 = 0x6449_6e20; // 'dIn '
    const K_AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE: u32 = 0x676f_6e65; // 'gone'
    const K_AUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST: u32 = 0x7072_7323; // 'prs#'
    const K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_INPUT: u32 = 0x7069_7269; // 'piri'
    const K_AUDIO_PROCESS_PROPERTY_PID: u32 = 0x7070_6964; // 'ppid'

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        m_selector: u32,
        m_scope: u32,
        m_element: u32,
    }

    impl AudioObjectPropertyAddress {
        fn global(selector: u32) -> Self {
            Self {
                m_selector: selector,
                m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            }
        }
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            in_object_id: AudioObjectID,
            in_address: *const AudioObjectPropertyAddress,
            in_qualifier_data_size: u32,
            in_qualifier_data: *const c_void,
            io_data_size: *mut u32,
            out_data: *mut c_void,
        ) -> OSStatus;

        fn AudioObjectGetPropertyDataSize(
            in_object_id: AudioObjectID,
            in_address: *const AudioObjectPropertyAddress,
            in_qualifier_data_size: u32,
            in_qualifier_data: *const c_void,
            out_data_size: *mut u32,
        ) -> OSStatus;
    }

    // ---------------------------------------------------------------------------
    // CoreAudio helpers
    // ---------------------------------------------------------------------------

    fn default_input_device() -> Option<AudioObjectID> {
        let addr = AudioObjectPropertyAddress::global(K_AUDIO_HARDWARE_PROPERTY_DEFAULT_INPUT_DEVICE);
        let mut device_id: AudioObjectID = 0;
        let mut size = std::mem::size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut device_id as *mut _ as *mut c_void,
            )
        };
        if status == 0 && device_id != 0 { Some(device_id) } else { None }
    }

    fn device_is_running_somewhere(device_id: AudioObjectID) -> bool {
        let addr = AudioObjectPropertyAddress::global(K_AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE);
        let mut running: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut running as *mut _ as *mut c_void,
            )
        };
        status == 0 && running != 0
    }

    // macOS 14+ only — returns PIDs of processes currently capturing input.
    fn pids_capturing_input() -> HashSet<i32> {
        let list_addr = AudioObjectPropertyAddress::global(K_AUDIO_HARDWARE_PROPERTY_PROCESS_OBJECT_LIST);
        let mut size: u32 = 0;

        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &list_addr,
                0,
                std::ptr::null(),
                &mut size,
            )
        };
        if status != 0 || size == 0 {
            return HashSet::new();
        }

        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut processes: Vec<AudioObjectID> = vec![0; count];
        let status = unsafe {
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &list_addr,
                0,
                std::ptr::null(),
                &mut size,
                processes.as_mut_ptr() as *mut c_void,
            )
        };
        if status != 0 {
            return HashSet::new();
        }

        let input_addr = AudioObjectPropertyAddress::global(K_AUDIO_PROCESS_PROPERTY_IS_RUNNING_INPUT);
        let pid_addr = AudioObjectPropertyAddress::global(K_AUDIO_PROCESS_PROPERTY_PID);

        let mut pids = HashSet::new();
        for proc_id in processes {
            let mut is_running: u32 = 0;
            let mut s1 = std::mem::size_of::<u32>() as u32;
            let ok = unsafe {
                AudioObjectGetPropertyData(
                    proc_id,
                    &input_addr,
                    0,
                    std::ptr::null(),
                    &mut s1,
                    &mut is_running as *mut _ as *mut c_void,
                )
            } == 0;
            if !ok || is_running == 0 {
                continue;
            }

            let mut pid: i32 = 0;
            let mut s2 = std::mem::size_of::<i32>() as u32;
            let ok = unsafe {
                AudioObjectGetPropertyData(
                    proc_id,
                    &pid_addr,
                    0,
                    std::ptr::null(),
                    &mut s2,
                    &mut pid as *mut _ as *mut c_void,
                )
            } == 0;
            if ok {
                pids.insert(pid);
            }
        }
        pids
    }

    // Returns (bundle_id, app_name) for a PID using NSRunningApplication.
    fn app_info(pid: i32) -> (Option<String>, Option<String>) {
        unsafe {
            match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
                Some(app) => (
                    app.bundleIdentifier().map(|s| s.to_string()),
                    app.localizedName().map(|s: objc2::rc::Retained<NSString>| s.to_string()),
                ),
                None => (None, None),
            }
        }
    }

    fn humanize(bundle_id: Option<&str>, app_name: Option<&str>) -> String {
        if let Some(id) = bundle_id {
            if id.contains("WebKit") { return "Safari".to_string(); }
            if id.contains("google.Chrome") { return "Google Chrome".to_string(); }
            if id.contains("chromium") { return "Chromium".to_string(); }
            if id.contains("microsoft.edgemac") { return "Microsoft Edge".to_string(); }
            if id.contains("thebrowser.Browser") { return "Arc".to_string(); }
            if id.contains("brave.Browser") { return "Brave".to_string(); }
            if id.contains("mozilla") { return "Firefox".to_string(); }
        }
        app_name.unwrap_or("Unknown App").to_string()
    }

    // ---------------------------------------------------------------------------
    // Public surface
    // ---------------------------------------------------------------------------

    const MEETILY_BUNDLE_ID: &str = "com.meetily.ai";
    const MEETING_END_DEBOUNCE_MS: u64 = 3_000;
    const FALLBACK_PID: i32 = -1;

    static ENABLED: AtomicBool = AtomicBool::new(true);

    pub fn set_enabled(enabled: bool) {
        ENABLED.store(enabled, Ordering::SeqCst);
        log::info!("[auto-detect] {}", if enabled { "enabled" } else { "disabled" });
    }

    pub fn start<R: Runtime + 'static>(app: &AppHandle<R>) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            run_loop(app).await;
        });
    }

    async fn run_loop<R: Runtime>(app: AppHandle<R>) {
        let macos_ver = macos_version();
        log::info!(
            "[auto-detect] run_loop started — macOS {}.{}, using {} API",
            macos_ver.0,
            macos_ver.1,
            if macos_ver >= (14, 0) { "process-list" } else { "device-fallback" },
        );

        // State shared across poll ticks.
        let mut last_pids: HashSet<i32> = HashSet::new();
        let mut pid_cache: HashMap<i32, (Option<String>, Option<String>)> = HashMap::new();
        let mut prompted_app: Option<String> = None;
        let mut prompted_pid: Option<i32> = None;
        let mut stop_at: Option<tokio::time::Instant> = None;

        let macos_version = macos_ver;

        // Load auto-record app list; refresh every 30 ticks so settings changes
        // take effect without restarting the app.
        let mut auto_record_apps = crate::audio::recording_preferences::load_recording_preferences(&app)
            .await
            .unwrap_or_default()
            .auto_record_apps;
        let mut ticks_since_prefs_reload: u32 = 0;

        loop {
            if !ENABLED.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            let stop_deadline = async {
                match stop_at {
                    Some(t) => tokio::time::sleep_until(t).await,
                    None => std::future::pending::<()>().await,
                }
            };

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    ticks_since_prefs_reload += 1;
                    if ticks_since_prefs_reload >= 30 {
                        ticks_since_prefs_reload = 0;
                        if let Ok(prefs) = crate::audio::recording_preferences::load_recording_preferences(&app).await {
                            auto_record_apps = prefs.auto_record_apps;
                        }
                    }
                    poll_tick(
                        &app,
                        macos_version,
                        &mut last_pids,
                        &mut pid_cache,
                        &mut prompted_app,
                        &mut prompted_pid,
                        &mut stop_at,
                        &auto_record_apps,
                    );
                }

                _ = stop_deadline, if stop_at.is_some() => {
                    log::info!("[auto-detect] meeting ended, stopping recording");
                    stop_at = None;
                    prompted_app = None;
                    prompted_pid = None;
                    let app_c = app.clone();
                    tauri::async_runtime::spawn(async move {
                        auto_stop(app_c).await;
                    });
                }
            }
        }
    }

    fn poll_tick<R: Runtime>(
        app: &AppHandle<R>,
        macos_version: (u32, u32),
        last_pids: &mut HashSet<i32>,
        pid_cache: &mut HashMap<i32, (Option<String>, Option<String>)>,
        prompted_app: &mut Option<String>,
        prompted_pid: &mut Option<i32>,
        stop_at: &mut Option<tokio::time::Instant>,
        auto_record_apps: &[crate::audio::recording_preferences::AutoRecordApp],
    ) {
        let using_process_api = macos_version >= (14, 0);
        let current_pids: HashSet<i32> = if using_process_api {
            pids_capturing_input()
        } else if let Some(dev) = default_input_device() {
            if device_is_running_somewhere(dev) {
                [FALLBACK_PID].into()
            } else {
                HashSet::new()
            }
        } else {
            HashSet::new()
        };

        // Log every tick so we can see whether detection is even running.
        log::debug!(
            "[auto-detect] tick — macOS {}.{} api={} pids={:?} last={:?}",
            macos_version.0,
            macos_version.1,
            if using_process_api { "process-list" } else { "device-fallback" },
            current_pids,
            last_pids,
        );

        if !current_pids.is_empty() {
            log::info!("[auto-detect] mic-capturing PIDs: {:?}", current_pids);
        }

        // New pids started capturing.
        for &pid in current_pids.difference(last_pids) {
            let info = if pid == FALLBACK_PID {
                (None, None)
            } else {
                app_info(pid)
            };
            pid_cache.insert(pid, info.clone());

            let (bundle_id, app_name) = &info;

            log::info!(
                "[auto-detect] new PID {} capturing — bundle_id={:?} app_name={:?}",
                pid,
                bundle_id,
                app_name,
            );

            // Skip ourselves.
            if bundle_id.as_deref() == Some(MEETILY_BUNDLE_ID) {
                log::info!("[auto-detect] skipping own PID {}", pid);
                continue;
            }

            // Skip processes with no identifiable bundle or name — these are
            // typically audio daemons or system processes, not meeting apps.
            if bundle_id.is_none() && app_name.is_none() {
                log::info!("[auto-detect] skipping PID {} — no bundle ID or app name", pid);
                continue;
            }

            // If the same app resumed, cancel a pending stop.
            if prompted_app.as_deref() == bundle_id.as_deref() && prompted_app.is_some() {
                if stop_at.take().is_some() {
                    log::info!("[auto-detect] meeting resumed, cancelling stop debounce");
                }
                continue;
            }

            let display_name = humanize(bundle_id.as_deref(), app_name.as_deref());
            let meeting_name = format!(
                "Meeting with {} – {}",
                display_name,
                Local::now().format("%b %-d %H:%M")
            );

            *prompted_app = bundle_id.clone().or_else(|| Some("__fallback__".to_string()));
            *prompted_pid = Some(pid);
            *stop_at = None;

            let is_auto_record = bundle_id
                .as_deref()
                .map(|id| auto_record_apps.iter().any(|a| a.bundle_id == id))
                .unwrap_or(false);

            if is_auto_record {
                log::info!(
                    "[auto-detect] detected '{}' ({}) — in auto-record list, starting immediately",
                    display_name,
                    bundle_id.as_deref().unwrap_or("no-bundle-id")
                );
                crate::overlay::show_recording(app, &meeting_name);
                let app_c = app.clone();
                let name = meeting_name.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::audio::recording_commands::start_recording_with_meeting_name(
                        app_c,
                        Some(name),
                    )
                    .await;
                });
            } else {
                log::info!(
                    "[auto-detect] detected '{}' ({}), prompting user",
                    display_name,
                    bundle_id.as_deref().unwrap_or("no-bundle-id")
                );
                crate::overlay::show_prompt(app, &display_name, &meeting_name);
            }
        }

        // Pids that stopped capturing.
        for &pid in last_pids.difference(&current_pids) {
            let info = pid_cache.remove(&pid).unwrap_or((None, None));
            let (bundle_id, _) = &info;

            // Only care about the app we prompted for.
            let matches = match (prompted_app.as_deref(), bundle_id.as_deref()) {
                (Some("__fallback__"), None) => true,
                (Some(a), Some(b)) => a == b,
                (Some(_), None) => prompted_pid.is_some() && *prompted_pid == Some(pid),
                _ => false,
            };
            if !matches {
                continue;
            }

            *stop_at = Some(
                tokio::time::Instant::now() + Duration::from_millis(MEETING_END_DEBOUNCE_MS),
            );
            log::info!(
                "[auto-detect] mic released, stop debounce armed ({}ms)",
                MEETING_END_DEBOUNCE_MS
            );
        }

        *last_pids = current_pids;
    }

    async fn auto_stop<R: Runtime>(app: AppHandle<R>) {
        let args = crate::audio::recording_commands::RecordingArgs {
            save_path: String::new(),
        };
        match crate::audio::recording_commands::stop_recording(app.clone(), args).await {
            Ok(_) => {
                // Signal the frontend to run post-stop processing (save + summarize).
                // recording-stopped (with folder_path/meeting_name) is emitted inside
                // stop_recording above and arrives first, so sessionStorage is already
                // populated when this event is handled.
                let _ = app.emit("recording-auto-stopped", ());
            }
            Err(e) if e.contains("No recording") => {
                // Recording was already stopped by another path (e.g. overlay button).
                // Do NOT emit recording-auto-stopped — the other path already triggered
                // post-stop processing and emitting here would cause a duplicate save.
                log::info!("[auto-detect] recording already stopped, skipping auto-stopped event");
            }
            Err(e) => {
                log::error!("[auto-detect] stop_recording failed: {}", e);
            }
        }
        crate::overlay::close(&app);
        crate::tray::update_tray_menu(&app);
    }

    fn macos_version() -> (u32, u32) {
        use std::process::Command;
        let out = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let mut parts = out.trim().splitn(3, '.');
        let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u32);
        let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u32);
        (major, minor)
    }
}

// Public surface — only compiled on macOS.
#[cfg(target_os = "macos")]
pub use macos::{set_enabled, start};

// Stubs for non-macOS targets so the rest of the codebase compiles unchanged.
#[cfg(not(target_os = "macos"))]
pub fn start<R: tauri::Runtime + 'static>(_app: &tauri::AppHandle<R>) {}

#[cfg(not(target_os = "macos"))]
pub fn set_enabled(_enabled: bool) {}
