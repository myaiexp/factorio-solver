// Which commit this binary was built from, and whether the checkout it lives
// in has since moved past it.
//
// The stamp answers a question that can only be asked from inside the app:
// `scripts/launch.sh` launches the *previous* binary when a build fails, and
// that fallback looks exactly like a healthy launch — same window, same data,
// old code. The build-failure notification is a toast in a terminal that
// closes; this is on screen for as long as the app is.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[cfg(test)]
#[path = "build_info_tests.rs"]
mod tests;

/// Stamped by `build.rs`. Empty when built outside a git checkout.
const COMMIT: &str = env!("FS_BUILD_COMMIT");
const DATE: &str = env!("FS_BUILD_DATE");

/// The full commit sha this binary was built from.
pub fn commit() -> Option<&'static str> {
    non_empty(COMMIT)
}

/// The commit's author date, `YYYY-MM-DD`.
pub fn date() -> Option<&'static str> {
    non_empty(DATE)
}

/// The first seven characters — git's own default abbreviation, and short
/// enough to sit in a status bar. Sliced by byte index safely because a sha is
/// ASCII hex; `min` covers a stamp that is somehow shorter.
pub fn short_commit() -> Option<&'static str> {
    commit().map(|c| &c[..c.len().min(7)])
}

/// The status-bar line: `build c0bcecf · 2026-08-13`.
pub fn label() -> String {
    format_label(short_commit(), date())
}

/// Split out from [`label`] so the no-stamp and no-date shapes are testable:
/// the consts are fixed at compile time, so a test driving `label()` can only
/// ever exercise whichever branch this machine's build happened to take.
fn format_label(commit: Option<&str>, date: Option<&str>) -> String {
    match (commit, date) {
        (Some(commit), Some(date)) => format!("build {commit} · {date}"),
        (Some(commit), None) => format!("build {commit}"),
        // No commit means no build to name, whatever the date says.
        (None, _) => "build unknown".to_string(),
    }
}

fn non_empty(text: &'static str) -> Option<&'static str> {
    (!text.is_empty()).then_some(text)
}

/// How the running binary compares to the checkout it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// The checkout's HEAD is the commit this binary was built from.
    Current,
    /// The checkout has moved on — the binary is not the checked-out code.
    /// `behind` is the number of commits added since, when git could count
    /// them; `None` when the built commit is not in this repository at all
    /// (a rebase, a different clone), where the difference is still real but
    /// no longer a distance.
    Stale { behind: Option<u32> },
    /// Nothing to compare against: no stamp, no git, or a binary copied out
    /// of its checkout. Deliberately not conflated with `Current` — "I cannot
    /// tell" and "you are up to date" are different answers.
    Unknown,
}

impl Freshness {
    /// The warning line, or `None` when there is nothing to warn about.
    /// Separate from `label()` because the two are painted differently: the
    /// stamp is always dim, this is always loud.
    pub fn warning(&self) -> Option<String> {
        match self {
            Freshness::Current | Freshness::Unknown => None,
            Freshness::Stale { behind: Some(1) } => {
                Some("1 commit behind the checkout".to_string())
            }
            Freshness::Stale { behind: Some(n) } => {
                Some(format!("{n} commits behind the checkout"))
            }
            Freshness::Stale { behind: None } => {
                Some("not the checked-out commit".to_string())
            }
        }
    }
}

/// Compare the stamped commit against the HEAD of the repository containing
/// `dir`. Blocking — it shells out to git twice; run it off the UI thread.
///
/// Shelling out rather than parsing `.git` by hand: HEAD can be a symbolic
/// ref, a detached sha, or (in a linked worktree, which is how this repo is
/// developed) a file in `.git/worktrees/<name>/` whose refs live somewhere
/// else entirely. git already knows all of that.
pub fn check_in(dir: &Path) -> Freshness {
    match commit() {
        Some(built) => compare(dir, built),
        None => Freshness::Unknown,
    }
}

/// Split from [`check_in`] so tests can name the commit. The compiled-in one
/// is fixed at build time and — on any machine that can run these tests — is
/// this very checkout's HEAD, so a test driving `check_in` could only ever
/// exercise the `Current` arm.
fn compare(dir: &Path, built: &str) -> Freshness {
    let Some(head) = git_in(dir, &["rev-parse", "HEAD"]) else {
        return Freshness::Unknown;
    };
    if head == built {
        return Freshness::Current;
    }
    let behind = git_in(dir, &["rev-list", "--count", &format!("{built}..HEAD")])
        .and_then(|count| count.parse().ok());
    Freshness::Stale { behind }
}

/// The directory the running executable sits in — `<repo>/target/release` for
/// anything the launcher started, which is inside the checkout and so enough
/// for git to find it. A binary copied elsewhere resolves to no repository and
/// reports `Unknown`, which is the honest answer.
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// The environment variables that redirect git's repository discovery. Git
/// *exports* these to every hook it runs, so anything a hook starts inherits
/// them — and they outrank `current_dir` completely.
const REDIRECTING_GIT_VARS: [&str; 9] = [
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_PREFIX",
];

/// A git invocation that really does run against `dir`.
///
/// Stripping the variables above is not defensive tidiness. This workspace's
/// commit gate runs `cargo test` from a pre-commit hook, which means these
/// tests run with `GIT_DIR` and `GIT_INDEX_FILE` pointing at the repository
/// being committed to — and with them set, every lookup below answers about
/// *that* repository no matter which directory it was aimed at. It is not a
/// test-only hazard either: the same inheritance reaches anything launched
/// from a hook or a `git rebase --exec`, where a stale-build check would
/// silently report on the wrong repository.
fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    for var in REDIRECTING_GIT_VARS {
        cmd.env_remove(var);
    }
    cmd
}

fn git_in(dir: &Path, args: &[&str]) -> Option<String> {
    let out = git_command(dir).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Runs [`check_in`] once, on its own thread, and hands the answer to the UI.
///
/// Off the UI thread for the same reason the clipboard watcher is: git
/// round-trips to the filesystem and spawns a process, and a stall there is a
/// dropped frame on a path that runs before the first one is even painted.
pub struct FreshnessProbe {
    rx: Option<Receiver<Freshness>>,
    result: Option<Freshness>,
}

impl FreshnessProbe {
    pub fn spawn(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        // Detached like the clipboard watcher's: it holds nothing the app
        // needs back and finishes in milliseconds.
        std::thread::Builder::new()
            .name("build-freshness".into())
            .spawn(move || {
                let freshness = exe_dir().map_or(Freshness::Unknown, |dir| check_in(&dir));
                // A failed send only means the app is already gone.
                if tx.send(freshness).is_ok() {
                    ctx.request_repaint();
                }
            })
            .expect("spawn build-freshness probe");

        Self { rx: Some(rx), result: None }
    }

    /// A probe with no thread behind it, answering immediately. Tests must not
    /// depend on the machine's git state — the answer on a dev checkout and
    /// the answer in a sandbox are different by construction.
    #[cfg(test)]
    pub(crate) fn ready(freshness: Freshness) -> Self {
        Self { rx: None, result: Some(freshness) }
    }

    /// A probe that never answers — the state every real launch starts in,
    /// and the one the status bar has to render without claiming anything.
    #[cfg(test)]
    pub(crate) fn pending() -> Self {
        Self { rx: None, result: None }
    }

    /// The answer once it arrives, cached from then on; `None` while the probe
    /// is still running. Called every frame, so it must never block.
    pub fn get(&mut self) -> Option<&Freshness> {
        if self.result.is_none() {
            let incoming = match self.rx.as_ref().map(Receiver::try_recv) {
                Some(Ok(freshness)) => Some(freshness),
                // The thread died without answering — say so rather than
                // polling a dead channel for the rest of the run.
                Some(Err(TryRecvError::Disconnected)) => Some(Freshness::Unknown),
                Some(Err(TryRecvError::Empty)) | None => None,
            };
            if let Some(freshness) = incoming {
                self.result = Some(freshness);
                self.rx = None;
            }
        }
        self.result.as_ref()
    }
}
