//! Calendar meetings through EventKit: every account added in System Settings, one permission.
//! Only read; asked for only from the setup screen or Settings.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use chrono::{DateTime, Utc};
use muesli_core::ports::Access;
use muesli_core::{DetectedMeeting, DetectionLogic};
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
use objc2_foundation::{NSDate, NSError};

thread_local! {
    // An event store is expensive to create and not thread-safe; detection always samples from
    // the same thread, so each thread keeps its own, with the grant it was made under.
    static STORE: RefCell<(u32, Retained<EKEventStore>)> = RefCell::new((0, unsafe { EKEventStore::new() }));
}

/// Set when the system's question was answered with Allow in this run. EventKit's
/// `authorizationStatusForEntityType` keeps answering "not determined" in the running process
/// after that, so Settings would go on offering Allow….
static GRANTED: AtomicBool = AtomicBool::new(false);
/// Bumped by a grant: a store made before it cannot read events, so each thread makes a new one.
static GRANT: AtomicU32 = AtomicU32::new(0);

/// This thread's event store, made again after a grant.
fn with_store<T>(f: impl FnOnce(&EKEventStore) -> T) -> T {
    STORE.with(|cell| {
        let mut cell = cell.borrow_mut();
        let grant = GRANT.load(Ordering::SeqCst);
        if cell.0 != grant {
            *cell = (grant, unsafe { EKEventStore::new() });
        }
        f(&cell.1)
    })
}

fn date(d: DateTime<Utc>) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(d.timestamp() as f64)
}

pub fn access() -> Access {
    match unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) } {
        EKAuthorizationStatus::FullAccess => Access::Allowed,
        EKAuthorizationStatus::NotDetermined if GRANTED.load(Ordering::SeqCst) => Access::Allowed,
        EKAuthorizationStatus::NotDetermined => Access::NotAsked,
        EKAuthorizationStatus::Denied | EKAuthorizationStatus::Restricted | EKAuthorizationStatus::WriteOnly => Access::Denied,
        _ => Access::Unknown,
    }
}

/// Shows the system's calendar question and waits (up to a minute) for the answer.
pub fn request() -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });
    with_store(|store| unsafe { store.requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&block)) });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(true) => {
            GRANTED.store(true, Ordering::SeqCst);
            GRANT.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        Ok(false) => Err("Calendar access was not allowed.".into()),
        Err(_) => Err("No answer to the calendar question.".into()),
    }
}

/// The calendar meeting running now, with an id that is unique per occurrence.
pub fn meeting_now(now: DateTime<Utc>) -> Option<(DetectedMeeting, String)> {
    if access() != Access::Allowed {
        return None;
    }
    with_store(|store| unsafe {
        let start = date(now - chrono::Duration::hours(12));
        let end = date(now + chrono::Duration::minutes(1));
        let predicate = store.predicateForEventsWithStartDate_endDate_calendars(&start, &end, None);
        let events = store.eventsMatchingPredicate(&predicate);
        let now_s = now.timestamp() as f64;
        for event in events.iter() {
            if event.isAllDay() || event.startDate().timeIntervalSince1970() > now_s || event.endDate().timeIntervalSince1970() <= now_s {
                continue;
            }
            let title = event.title().to_string();
            let mut text = title.clone();
            for extra in [event.location(), event.notes()].into_iter().flatten() {
                text.push(' ');
                text.push_str(&extra.to_string());
            }
            if let Some(url) = event.URL().and_then(|u| u.absoluteString()) {
                text.push(' ');
                text.push_str(&url.to_string());
            }
            let attendees: Vec<String> = event
                .attendees()
                .map(|list| list.iter().filter(|p| !p.isCurrentUser()).filter_map(|p| p.name()).map(|n| n.to_string()).collect())
                .unwrap_or_default();
            // Attendees other than the user, plus the user: the count the detection rules expect.
            if !DetectionLogic::looks_like_meeting(&text, attendees.len() + 1) {
                continue;
            }
            let id = format!(
                "{}@{}",
                event.eventIdentifier().map(|i| i.to_string()).unwrap_or_else(|| title.clone()),
                event.startDate().timeIntervalSince1970() as i64
            );
            let mut meeting = DetectedMeeting::new(if title.trim().is_empty() { "Meeting".to_string() } else { title }, "calendar");
            meeting.attendees = attendees;
            return Some((meeting, id));
        }
        None
    })
}
