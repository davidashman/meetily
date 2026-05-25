// Small always-on-top overlay window shown bottom-right.
//
// States
//   prompt    — mic detected, ask user to record or decline
//   recording — recording in progress, show timer + Stop button
//
// The overlay transitions from prompt → recording when the user clicks
// Record, and disappears when recording stops (from any source).

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "mic-overlay";
const WINDOW_W: f64 = 160.0;
const WINDOW_H: f64 = 118.0;
const MARGIN: f64 = 20.0;

// ---------------------------------------------------------------------------
// Public helpers called from lib.rs / automation
// ---------------------------------------------------------------------------

/// Show the overlay in "prompt" mode (mic detected, ask to record).
/// Safe to call from any thread.
pub fn show_prompt<R: tauri::Runtime + 'static>(
    app: &tauri::AppHandle<R>,
    display_name: &str,
    meeting_name: &str,
) {
    let app = app.clone();
    let display_name = display_name.to_string();
    let meeting_name = meeting_name.to_string();

    tauri::async_runtime::spawn(async move {
        close_existing(&app).await;
        let (x, y) = bottom_right(&app);
        let url = format!(
            "overlay.html?mode=prompt&app={}&meeting={}",
            url_encode(&display_name),
            url_encode(&meeting_name),
        );
        build_window(&app, &url, x, y);
    });
}

/// Show (or transition) the overlay in "recording" mode.
/// Called when recording starts from the main UI (not via the overlay button).
pub fn show_recording<R: tauri::Runtime + 'static>(
    app: &tauri::AppHandle<R>,
    meeting_name: &str,
) {
    let app = app.clone();
    let meeting_name = meeting_name.to_string();

    tauri::async_runtime::spawn(async move {
        if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
            // Overlay already open (prompt mode) — transition it in place.
            let _ = w.emit("overlay:state", serde_json::json!({
                "mode": "recording",
                "meetingName": meeting_name,
            }));
        } else {
            // No overlay yet — create one in recording mode.
            let (x, y) = bottom_right(&app);
            let url = format!(
                "overlay.html?mode=recording&meeting={}",
                url_encode(&meeting_name),
            );
            build_window(&app, &url, x, y);
        }
    });
}

/// Close the overlay window. Called when recording stops from any source.
pub fn close<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
    }
}

// ---------------------------------------------------------------------------
// Tauri commands called from the overlay page via invoke()
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn overlay_record(app: tauri::AppHandle<tauri::Wry>, meeting_name: String) {
    log::info!("[overlay] Record clicked for '{}'", meeting_name);

    match crate::audio::recording_commands::start_recording_with_meeting_name(
        app.clone(),
        Some(meeting_name.clone()),
    )
    .await
    {
        Ok(_) => {
            if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
                let _ = w.emit("overlay:state", serde_json::json!({
                    "mode": "recording",
                    "meetingName": meeting_name,
                }));
            }
        }
        Err(e) if e.contains("already in progress") => {
            // Already recording — just transition the overlay.
            if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
                let _ = w.emit("overlay:state", serde_json::json!({
                    "mode": "recording",
                    "meetingName": meeting_name,
                }));
            }
        }
        Err(e) => {
            log::error!("[overlay] start_recording failed: {}", e);
            close(&app);
        }
    }
}

#[tauri::command]
pub async fn overlay_decline(app: tauri::AppHandle<tauri::Wry>) {
    log::info!("[overlay] Decline clicked");
    close(&app);
}

#[tauri::command]
pub async fn overlay_stop(app: tauri::AppHandle<tauri::Wry>) {
    log::info!("[overlay] Stop clicked");
    let args = crate::audio::recording_commands::RecordingArgs {
        save_path: String::new(),
    };
    match crate::audio::recording_commands::stop_recording(app.clone(), args).await {
        Ok(_) => {
            if let Err(e) = app.emit("recording-stop-complete", true) {
                log::error!("[overlay] Failed to emit recording-stop-complete: {}", e);
            }
            close(&app);
        }
        Err(e) => {
            log::error!("[overlay] stop_recording failed: {}", e);
            close(&app); // close anyway
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn build_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>, url: &str, x: f64, y: f64) {
    match WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App(url.into()))
        .title("")
        .inner_size(WINDOW_W, WINDOW_H)
        .position(x, y)
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .focused(false)
        .accept_first_mouse(true)
        .skip_taskbar(true)
        .transparent(true)
        .build()
    {
        Ok(_) => log::info!("[overlay] window created at ({:.0}, {:.0})", x, y),
        Err(e) => log::error!("[overlay] failed to create window: {}", e),
    }
}

async fn close_existing<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
}

fn bottom_right<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> (f64, f64) {
    if let Ok(Some(m)) = app.primary_monitor() {
        let sf = m.scale_factor();
        let w = m.size().width as f64 / sf;
        let h = m.size().height as f64 / sf;
        (w - WINDOW_W - MARGIN, h - WINDOW_H - MARGIN)
    } else {
        (MARGIN, MARGIN)
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
