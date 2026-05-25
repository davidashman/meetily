// macOS actionable notifications via UNUserNotificationCenter.
// Replaces the deprecated notify-rust/NSUserNotificationCenter path used by
// tauri-plugin-notification on desktop, which cannot show action buttons.
//
// Flow:
//   1. Call `setup()` once at app startup — requests authorization, registers
//      the "MEETILY_MEETING_DETECTED" category with a "Start Recording" action,
//      installs the delegate, and returns an `UnboundedReceiver<String>` that
//      yields meeting names when the user clicks "Start Recording".
//   2. Call `show_meeting_prompt(app_name, meeting_name)` each time the
//      mic-monitor sidecar detects a new app grabbing the microphone.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, ClassType};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationAction,
    UNNotificationActionOptionNone, UNNotificationCategory, UNNotificationCategoryOptionNone,
    UNNotificationContent, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNNotificationTrigger, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};
use tokio::sync::mpsc;

const CATEGORY_ID: &str = "MEETILY_MEETING_DETECTED";
const ACTION_START: &str = "MEETILY_START_RECORDING";

// Sender half of the action channel — written by the ObjC delegate callback.
static ACTION_TX: OnceLock<mpsc::UnboundedSender<String>> = OnceLock::new();

// Maps notification identifier → meeting_name so the delegate can look it up
// without encoding it into userInfo (avoids NSDictionary boilerplate).
static PENDING: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

// Keeps the delegate alive for the lifetime of the process.
static DELEGATE: OnceLock<Retained<MeetilyNotifDelegate>> = OnceLock::new();

// Set to true once the authorization callback confirms the user granted permission.
static AUTHORIZED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Delegate class
// ---------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MeetilyNotifDelegate"]
    struct MeetilyNotifDelegate;

    unsafe impl NSObjectProtocol for MeetilyNotifDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for MeetilyNotifDelegate {
        // Show banner + sound even when Meetily is in the foreground.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &objc2_user_notifications::UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler.call((
                UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::Sound,
            ));
        }

        // User tapped an action button (or the notification itself).
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive_response(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let action_id = unsafe { response.actionIdentifier().to_string() };

            if action_id == ACTION_START {
                let notif_id = unsafe {
                    response
                        .notification()
                        .request()
                        .identifier()
                        .to_string()
                };

                if let Some(map) = PENDING.get() {
                    if let Ok(mut guard) = map.lock() {
                        if let Some(meeting_name) = guard.remove(&notif_id) {
                            if let Some(tx) = ACTION_TX.get() {
                                let _ = tx.send(meeting_name);
                            }
                        }
                    }
                }
            }

            completion_handler.call(());
        }
    }
);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns the current macOS notification authorization status as a string:
/// "authorized", "denied", "not_determined", or "provisional".
/// Bridges the async ObjC completion handler to a synchronous Rust call.
pub async fn get_authorization_status() -> &'static str {
    use std::sync::Arc;
    use tokio::sync::oneshot;
    use objc2_user_notifications::UNAuthorizationStatus;

    let (tx, rx) = oneshot::channel::<UNAuthorizationStatus>();
    let tx = Arc::new(std::sync::Mutex::new(Some(tx)));

    unsafe {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let tx_clone = tx.clone();
        let handler = RcBlock::new(move |settings: std::ptr::NonNull<objc2_user_notifications::UNNotificationSettings>| {
            let status = settings.as_ref().authorizationStatus();
            if let Some(sender) = tx_clone.lock().ok().and_then(|mut g| g.take()) {
                let _ = sender.send(status);
            }
        });
        center.getNotificationSettingsWithCompletionHandler(&handler);
    }

    match rx.await {
        Ok(status) if status == UNAuthorizationStatus::Authorized => "authorized",
        Ok(status) if status == UNAuthorizationStatus::Denied => "denied",
        Ok(status) if status == UNAuthorizationStatus::Provisional => "provisional",
        _ => "not_determined",
    }
}

/// Explicitly set the authorization flag — called from the automation loop
/// after checking current status at startup.
pub fn set_authorized(value: bool) {
    AUTHORIZED.store(value, Ordering::SeqCst);
}

/// One-time setup. Call from `automation::start` before spawning the loop.
/// Returns a channel receiver that yields meeting names when the user clicks
/// "Start Recording" in a system notification.
pub fn setup() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let _ = ACTION_TX.set(tx);
    PENDING.get_or_init(|| Mutex::new(HashMap::new()));

    unsafe {
        // Log the bundle identifier the process is running under — UNUserNotificationCenter
        // needs a properly-bundled app with a valid CFBundleIdentifier, otherwise the
        // permission request silently no-ops and the app never appears in System Settings.
        let main_bundle = objc2_foundation::NSBundle::mainBundle();
        let bundle_id = main_bundle
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "<nil>".to_string());
        let bundle_path = main_bundle.bundlePath().to_string();
        log::info!("[macos-notif] setup() — bundleIdentifier={} bundlePath={}", bundle_id, bundle_path);

        let center = UNUserNotificationCenter::currentNotificationCenter();

        // Register category with a "Start Recording" action button.
        let action = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str(ACTION_START),
            &NSString::from_str("Start Recording"),
            UNNotificationActionOptionNone,
        );
        let actions = NSArray::from_retained_slice(&[action]);
        let intent_ids = NSArray::<NSString>::new();
        let category =
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &NSString::from_str(CATEGORY_ID),
                &actions,
                &intent_ids,
                UNNotificationCategoryOptionNone,
            );
        let categories = NSSet::from_retained_slice(&[category]);
        center.setNotificationCategories(&categories);

        // Request authorization (alert + sound). This is what makes the app
        // appear in System Settings -> Notifications on first launch.
        // On subsequent runs, returns the cached grant/deny without showing a dialog.
        let opts = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
        let handler = RcBlock::new(|granted: Bool, err: *mut NSError| {
            if !err.is_null() {
                let desc = unsafe { (*err).localizedDescription().to_string() };
                log::error!("[macos-notif] authorization request failed: {}", desc);
            } else if granted.as_bool() {
                AUTHORIZED.store(true, Ordering::SeqCst);
                log::info!("[macos-notif] notification permission granted");
            } else {
                log::warn!("[macos-notif] notification permission DENIED — enable in System Settings > Notifications > Meetily");
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(opts, &handler);

        // Create delegate and set it on the notification center.
        let delegate: Retained<MeetilyNotifDelegate> =
            Retained::from_raw(msg_send![MeetilyNotifDelegate::class(), new])
                .expect("failed to allocate MeetilyNotifDelegate");

        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let _ = DELEGATE.set(delegate);
    }

    rx
}

/// Show a "meeting detected" system notification with a Start Recording button.
pub fn show_meeting_prompt(app_name: &str, meeting_name: &str) {
    if !AUTHORIZED.load(Ordering::SeqCst) {
        log::warn!(
            "[macos-notif] skipping notification for '{}' — not authorized yet (grant in System Settings > Notifications > Meetily)",
            app_name
        );
        return;
    }

    let notif_id = uuid::Uuid::new_v4().to_string();

    if let Some(map) = PENDING.get() {
        if let Ok(mut guard) = map.lock() {
            guard.insert(notif_id.clone(), meeting_name.to_string());
        }
    }

    unsafe {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&format!(
            "{} is using your microphone",
            app_name
        )));
        content.setBody(&NSString::from_str(
            "Tap \"Start Recording\" to record this meeting.",
        ));
        content.setCategoryIdentifier(&NSString::from_str(CATEGORY_ID));

        // nil trigger -> deliver immediately.
        let id_ns = NSString::from_str(&notif_id);
        let content_ref: &UNNotificationContent = &**content;
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &id_ns,
            content_ref,
            None::<&UNNotificationTrigger>,
        );

        let notif_id_log = notif_id.clone();
        let err_handler = RcBlock::new(move |err: *mut NSError| {
            if err.is_null() {
                log::info!("[macos-notif] notification posted: {}", notif_id_log);
            } else {
                let desc = unsafe { (*err).localizedDescription().to_string() };
                log::error!("[macos-notif] failed to post notification {}: {}", notif_id_log, desc);
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&*err_handler));
    }
}
