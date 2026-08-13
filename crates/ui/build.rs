// Stamps the commit this binary was built from into the executable.
//
// It exists because the desktop launcher falls back to the *previous* binary
// when a build fails (scripts/launch.sh), and a fallback build is
// indistinguishable from a current one from inside the app: one was found
// serving code 29 commits old, behind a build-failure notification that had
// long since flashed past. `src/build_info.rs` reads these two variables.
use std::path::Path;
use std::process::Command;

fn main() {
    // Empty rather than a placeholder string: `build_info` maps "" to "no
    // stamp", so a build outside a git checkout (a source tarball, a vendored
    // tree) reports "unknown" instead of inventing a commit.
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_default();
    let date = git(&["log", "-1", "--format=%cs"]).unwrap_or_default();
    println!("cargo:rustc-env=FS_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=FS_BUILD_DATE={date}");

    // Without these the stamp is baked once and then lies. Cargo caches a
    // build script's output, and once *any* rerun-if-changed is emitted it
    // reruns the script only when one of the named paths changes. Editing a
    // source file rebuilds the crate but not this script — which is correct,
    // since the commit has not moved — so what has to be watched is the commit
    // itself: HEAD (branch switches, detached checkouts), the file HEAD points
    // at (a new commit on the current branch), and packed-refs (for a ref that
    // lives packed rather than loose).
    for path in watch_paths() {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// The git files whose contents decide the stamp, resolved through git itself
/// rather than by assembling `.git/...` by hand — this workspace is developed
/// from linked worktrees, where HEAD lives in `.git/worktrees/<name>/` while
/// refs stay in the common dir, and only git knows that split.
///
/// Only paths that exist are emitted: cargo treats a missing
/// rerun-if-changed target as "changed", which would rerun this script — and
/// relink the whole binary — on every single build.
fn watch_paths() -> Vec<String> {
    let head_ref = git(&["symbolic-ref", "--quiet", "HEAD"]);
    [
        git_path("HEAD"),
        head_ref.and_then(|r| git_path(&r)),
        git_path("packed-refs"),
    ]
    .into_iter()
    .flatten()
    .filter(|p| Path::new(p).exists())
    .collect()
}

fn git_path(name: &str) -> Option<String> {
    git(&["rev-parse", "--path-format=absolute", "--git-path", name])
}

/// The variables that redirect git's repository discovery. Git exports these
/// to every hook it runs, and this workspace's commit gate builds from a
/// pre-commit hook — inherited, they outrank `current_dir` and would stamp the
/// binary from whichever repository invoked the gate. Duplicated from
/// `src/build_info.rs` because a build script shares no code with its crate.
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

/// Run git in the crate's own directory and return its trimmed stdout, or
/// `None` for anything that is not a clean success — no git on PATH, not a
/// repository, an empty answer. Every caller here treats absence as "no
/// stamp", never as a build failure: the app must build without git.
fn git(args: &[&str]) -> Option<String> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut cmd = Command::new("git");
    cmd.current_dir(dir).args(args);
    for var in REDIRECTING_GIT_VARS {
        cmd.env_remove(var);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}
