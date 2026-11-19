//! Questions and notices through UNUserNotificationCenter, with buttons.
//!
//! "Start transcribing / Not now" and "Stop and save / Keep transcribing" are notification
//! categories with two actions; "Saved: …" has one ("Open"). The answer arrives through the
//! center's delegate, which looks up the waiting callback by the request id. When notifications
//! are off (or the app is not a signed bundle, as with `cargo run`), `ask` fails and the engine
//! asks in Muesli's prompt window instead.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use muesli_core::ports::{Access, Notice, Notifier, OnAction, OnAnswer, Question};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_foundation::{NSArray, NSError, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions, UNNotificationDefaultActionIdentifier,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse, UNNotificationSettings, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

const ACCEPT: &str = "accept";
const DECLINE: &str = "decline";
const OPEN: &str = "open";

enum Waiting {
    Answer(OnAnswer),
    Action(OnAction),
}

/// Callbacks waiting for a click, by request id.
fn waiting() -> &'static Mutex<HashMap<String, Waiting>> {
    static WAITING: OnceLock<Mutex<HashMap<String, Waiting>>> = OnceLock::new();
    WAITING.get_or_init(Default::default)
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the class implements no Drop.
    #[unsafe(super(NSObject))]
    #[name = "MuesliNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// Show Muesli's notifications even while one of its windows is in front.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &block2::DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            let action = response.actionIdentifier().to_string();
            let default = unsafe { UNNotificationDefaultActionIdentifier }.to_string();
            let found = waiting().lock().unwrap().remove(&id); // the lock is released here
            if let Some(found) = found {
                match found {
                    Waiting::Answer(on_answer) => match action.as_str() {
                        ACCEPT => on_answer(true),
                        DECLINE => on_answer(false),
                        _ => {
                            // A click on the text answers nothing: keep waiting.
                            waiting().lock().unwrap().insert(id, Waiting::Answer(on_answer));
                        }
                    },
                    Waiting::Action(on_action) => {
                        if action == OPEN || action == default {
                            on_action();
                        }
                    }
                }
            }
            completion.call(());
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// The `Notifier` adapter.
pub struct Notifications {
    next: AtomicU64,
}

/// Keeps the delegate alive: the center holds it weakly.
static DELEGATE: OnceLock<usize> = OnceLock::new();

/// UNUserNotificationCenter only works in an app bundle (it throws otherwise, e.g. under
/// `cargo run`); then Muesli asks in its own prompt window.
pub fn available() -> bool {
    static IN_BUNDLE: OnceLock<bool> = OnceLock::new();
    *IN_BUNDLE.get_or_init(|| objc2_foundation::NSBundle::mainBundle().bundleIdentifier().is_some())
}

fn center() -> Retained<UNUserNotificationCenter> {
    UNUserNotificationCenter::currentNotificationCenter()
}

fn action(id: &str, title: &str) -> Retained<UNNotificationAction> {
    UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str(id),
        &NSString::from_str(title),
        UNNotificationActionOptions::empty(),
    )
}

fn category(id: &str, actions: &[Retained<UNNotificationAction>]) -> Retained<UNNotificationCategory> {
    UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(id),
        &NSArray::from_retained_slice(actions),
        &NSArray::from_retained_slice(&[]),
        UNNotificationCategoryOptions::CustomDismissAction,
    )
}

impl Notifications {
    /// Installs the delegate. Call once, early (the app does it when it builds the platform).
    pub fn new() -> Self {
        if !available() {
            return Self { next: AtomicU64::new(1) };
        }
        DELEGATE.get_or_init(|| {
            let delegate = Delegate::new();
            center().setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            Retained::into_raw(delegate) as usize // lives as long as the app
        });
        Self { next: AtomicU64::new(1) }
    }

    fn show(&self, title: &str, body: &str, category_id: &str, actions: &[(&str, &str)], waiting_for: Waiting) -> Result<(), String> {
        if !matches!(access(), Access::Allowed) {
            return Err("Notifications are off for Muesli.".into());
        }
        let center = center();
        let built: Vec<Retained<UNNotificationAction>> = actions.iter().map(|(id, title)| action(id, title)).collect();
        // Categories carry the button titles, so each question registers its own.
        let category = category(category_id, &built);
        center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setCategoryIdentifier(&NSString::from_str(category_id));
        let id = format!("muesli-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&id), &content, None);
        waiting().lock().unwrap().insert(id.clone(), waiting_for);

        let (tx, rx) = mpsc::channel::<Option<String>>();
        let done = RcBlock::new(move |error: *mut NSError| {
            let message = unsafe { error.as_ref() }.map(|e| e.localizedDescription().to_string());
            let _ = tx.send(message);
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(None) => Ok(()),
            other => {
                waiting().lock().unwrap().remove(&id);
                Err(match other {
                    Ok(Some(why)) => why,
                    _ => "The notification could not be shown.".into(),
                })
            }
        }
    }
}

impl Default for Notifications {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for Notifications {
    fn ask(&self, question: &Question, on_answer: OnAnswer) -> Result<(), String> {
        self.show(
            &question.title,
            &question.body,
            &format!("question:{}:{}", question.accept, question.decline),
            &[(ACCEPT, &question.accept), (DECLINE, &question.decline)],
            Waiting::Answer(on_answer),
        )
    }

    fn notify(&self, notice: &Notice, on_action: OnAction) -> Result<(), String> {
        let actions: Vec<(&str, &str)> = notice.action.as_deref().map(|label| vec![(OPEN, label)]).unwrap_or_default();
        self.show(&notice.title, &notice.body, "notice", &actions, Waiting::Action(on_action))
    }
}

/// Whether macOS shows Muesli's notifications.
pub fn access() -> Access {
    if !available() {
        return Access::Unknown;
    }
    let (tx, rx) = mpsc::channel::<UNAuthorizationStatus>();
    let block = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        let _ = tx.send(unsafe { settings.as_ref() }.authorizationStatus());
    });
    center().getNotificationSettingsWithCompletionHandler(&block);
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(UNAuthorizationStatus::Authorized) | Ok(UNAuthorizationStatus::Provisional) | Ok(UNAuthorizationStatus::Ephemeral) => {
            Access::Allowed
        }
        Ok(UNAuthorizationStatus::NotDetermined) => Access::NotAsked,
        Ok(UNAuthorizationStatus::Denied) => Access::Denied,
        _ => Access::Unknown,
    }
}

/// Shows the system's notification question and waits (up to a minute) for the answer.
pub fn request() -> Result<(), String> {
    if !available() {
        return Err("Notifications need Muesli to run as an app bundle.".into());
    }
    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });
    center().requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &block);
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(true) => Ok(()),
        _ => Err("Notifications were not allowed.".into()),
    }
}
