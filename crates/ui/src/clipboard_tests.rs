use super::{ClipboardWatcher, Message, WatchStatus};

#[test]
fn an_empty_queue_offers_nothing() {
    let (_tx, mut watcher) = ClipboardWatcher::detached();
    assert_eq!(watcher.poll(), None);
}

/// The app can sit idle through several clipboard changes — minimised, or
/// simply not repainting. Replaying the backlog would flash each blueprint
/// through the viewport on the way to the one the user copied last.
#[test]
fn a_backlog_collapses_to_the_newest_blueprint() {
    let (tx, mut watcher) = ClipboardWatcher::detached();
    for s in ["first", "second", "third"] {
        tx.send(Message::Blueprint(s.into())).unwrap();
    }
    assert_eq!(watcher.poll().as_deref(), Some("third"));
    assert_eq!(watcher.poll(), None, "the queue is drained, not just read");
}

#[test]
fn an_unavailable_clipboard_is_reported_and_offers_no_blueprint() {
    let (tx, mut watcher) = ClipboardWatcher::detached();
    assert_eq!(watcher.status(), &WatchStatus::Running);

    tx.send(Message::Unavailable("no wlr-data-control".into())).unwrap();
    assert_eq!(watcher.poll(), None);
    assert_eq!(
        watcher.status(),
        &WatchStatus::Unavailable("no wlr-data-control".into())
    );
}

/// A failure arriving in the same drain as a blueprint must not swallow it:
/// the thread reports Unavailable only when `Clipboard::new()` fails and then
/// exits, so this is the shutdown case — whatever it managed to find first is
/// still worth loading.
#[test]
fn a_failure_in_the_same_drain_does_not_discard_the_blueprint() {
    let (tx, mut watcher) = ClipboardWatcher::detached();
    tx.send(Message::Blueprint("a-blueprint".into())).unwrap();
    tx.send(Message::Unavailable("backend died".into())).unwrap();

    assert_eq!(watcher.poll().as_deref(), Some("a-blueprint"));
    assert!(matches!(watcher.status(), WatchStatus::Unavailable(_)));
}

#[test]
fn toggling_off_is_visible_to_the_thread_and_to_the_ui() {
    let (_tx, mut watcher) = ClipboardWatcher::detached();
    assert!(watcher.watching());

    watcher.set_watching(false);
    assert!(!watcher.watching(), "the checkbox reads this");
    assert!(
        !watcher.enabled.load(std::sync::atomic::Ordering::Relaxed),
        "and the thread reads this — the two must not drift"
    );
}
