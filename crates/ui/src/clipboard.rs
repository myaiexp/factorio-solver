// Watches the system clipboard for a Factorio blueprint string, so an in-game
// export lands in the viewport with no paste step.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

pub mod detect;

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;

/// How often the watcher thread reads the clipboard. Fast enough that alt-tab
/// back from the game finds the blueprint already loaded, slow enough that the
/// round trip to the compositor costs nothing at two reads a second.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// What the watcher can tell the user about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchStatus {
    /// Polling normally.
    Running,
    /// No clipboard backend at all — the reason, verbatim from arboard. Worth
    /// showing: the alternative is a toggle that is on and does nothing, which
    /// reads as a bug rather than as a missing capability. On Wayland this is
    /// what a compositor without `wlr-data-control` looks like.
    Unavailable(String),
}

pub(crate) enum Message {
    Blueprint(String),
    Unavailable(String),
}

/// A background thread polling the clipboard, plus the toggle that stops it.
///
/// The read happens off the UI thread on purpose: `arboard`'s Wayland and X11
/// backends both round-trip to another process, and a stall there would show up
/// as a dropped frame. The thread pushes candidates over a channel and wakes
/// egui with `request_repaint`, so an idle app still notices one.
pub struct ClipboardWatcher {
    rx: mpsc::Receiver<Message>,
    /// Read by the thread each tick. Toggling off leaves the thread alive and
    /// idle rather than tearing down the clipboard connection, so turning it
    /// back on cannot fail in a way that a fresh `Clipboard::new()` could.
    enabled: Arc<AtomicBool>,
    watching: bool,
    status: WatchStatus,
}

impl ClipboardWatcher {
    pub fn spawn(ctx: &egui::Context, watching: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let enabled = Arc::new(AtomicBool::new(watching));

        let thread_ctx = ctx.clone();
        let thread_enabled = Arc::clone(&enabled);
        // Deliberately detached and never joined: it owns nothing the app needs
        // back, and it dies with the process when `run_native` returns. A join
        // handle would only buy an orderly shutdown of a thread that is asleep
        // 99.9% of the time.
        std::thread::Builder::new()
            .name("clipboard-watch".into())
            .spawn(move || watch_loop(thread_ctx, tx, thread_enabled))
            .expect("spawn clipboard watcher");

        Self { rx, enabled, watching, status: WatchStatus::Running }
    }

    /// A watcher with no thread behind it, fed by hand. Lets the drain rules
    /// below be tested without a display server — which the CI/agent machines
    /// this repo is worked on from do not have, so a test that spawned the
    /// real thread would assert one thing there and another on the desktop.
    #[cfg(test)]
    pub(crate) fn detached() -> (mpsc::Sender<Message>, Self) {
        let (tx, rx) = mpsc::channel();
        let watcher = Self {
            rx,
            enabled: Arc::new(AtomicBool::new(true)),
            watching: true,
            status: WatchStatus::Running,
        };
        (tx, watcher)
    }

    pub fn watching(&self) -> bool {
        self.watching
    }

    pub fn set_watching(&mut self, on: bool) {
        self.watching = on;
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn status(&self) -> &WatchStatus {
        &self.status
    }

    /// Drain everything the thread queued and return the newest blueprint it
    /// found, if any. Newest rather than oldest: if several arrived while the
    /// app was idle, replaying them would flash each one through the viewport
    /// on the way to the one the user actually copied last.
    pub fn poll(&mut self) -> Option<String> {
        let mut newest = None;
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Blueprint(s) => newest = Some(s),
                Message::Unavailable(reason) => self.status = WatchStatus::Unavailable(reason),
            }
        }
        newest
    }
}

fn watch_loop(ctx: egui::Context, tx: mpsc::Sender<Message>, enabled: Arc<AtomicBool>) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(e) => {
            let _ = tx.send(Message::Unavailable(e.to_string()));
            ctx.request_repaint();
            return;
        }
    };

    // Starts as None, so whatever is already on the clipboard at launch counts
    // as new and gets loaded. That is the point of the feature: copy in the
    // game, then start the app, and the blueprint is already there.
    let mut last_seen: Option<String> = None;

    loop {
        std::thread::sleep(POLL_INTERVAL);
        if !enabled.load(Ordering::Relaxed) {
            continue;
        }

        // A read error is the normal state of an empty clipboard, or one
        // holding an image — not a fault, and not worth reporting. Only
        // `Clipboard::new()` failing above means the feature cannot work.
        let Ok(text) = clipboard.get_text() else { continue };
        if last_seen.as_deref() == Some(text.as_str()) {
            continue;
        }
        last_seen = Some(text.clone());

        let candidate = text.trim();
        if !detect::looks_like_blueprint(candidate) {
            continue;
        }
        // Decoding here rather than on the UI thread keeps a 16 MiB paste off
        // the frame budget, and means the app is never handed something that
        // would surface as "Decode error" for clipboard text the user never
        // asked it to look at.
        if factorio_blueprint::decode(candidate).is_err() {
            continue;
        }

        if tx.send(Message::Blueprint(candidate.to_owned())).is_err() {
            return; // the app is gone
        }
        ctx.request_repaint();
    }
}
