// Tests for the build stamp and the checkout comparison.
use super::*;

/// The repository this crate is checked out in. Every git assertion below runs
/// against real history rather than against a repository the test builds,
/// because building one means running `git commit` — and a test that commits
/// is one environment slip away from committing into the developer's own
/// repository. It was, exactly once: the commit gate runs `cargo test` from a
/// pre-commit hook, git exports `GIT_DIR` and `GIT_INDEX_FILE` to its hooks,
/// and the temp repository's `git commit` landed a commit on the real branch
/// instead. `git_command` strips those variables now; this reads history and
/// writes none of it.
fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_label_names_the_commit_and_falls_back_to_unknown() {
    assert_eq!(format_label(Some("c0bcecf"), Some("2026-08-13")), "build c0bcecf · 2026-08-13");
    assert_eq!(format_label(Some("c0bcecf"), None), "build c0bcecf");
    assert_eq!(format_label(None, Some("2026-08-13")), "build unknown");
    assert_eq!(format_label(None, None), "build unknown");
}

/// The warning is what the user actually reads, and "1 commits behind" in a
/// message about a stale binary undercuts the one thing it has to be trusted
/// about.
#[test]
fn only_a_stale_build_warns_and_it_counts_in_english() {
    assert_eq!(Freshness::Current.warning(), None);
    assert_eq!(Freshness::Unknown.warning(), None);
    assert_eq!(
        Freshness::Stale { behind: Some(1) }.warning().as_deref(),
        Some("1 commit behind the checkout")
    );
    assert_eq!(
        Freshness::Stale { behind: Some(29) }.warning().as_deref(),
        Some("29 commits behind the checkout")
    );
    assert_eq!(
        Freshness::Stale { behind: None }.warning().as_deref(),
        Some("not the checked-out commit")
    );
}

/// The variables git hands its hooks outrank `current_dir` entirely, so every
/// lookup has to run with them gone. Asserted on the command rather than by
/// setting them in this process: `set_var` is process-wide and these tests
/// share it with every other thread.
#[test]
fn a_git_invocation_strips_the_variables_that_redirect_discovery() {
    let cmd = git_command(repo());
    let removed: Vec<&str> = cmd
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .filter_map(|(name, _)| name.to_str())
        .collect();
    for var in ["GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_COMMON_DIR"] {
        assert!(removed.contains(&var), "{var} must be stripped, got {removed:?}");
    }
}

/// A binary copied out of its checkout has nothing to compare against. This
/// must not read as `Current` — the whole point is that "I cannot tell" and
/// "you are up to date" are different answers.
///
/// Also the canary for the stripping above: with `GIT_DIR` inherited this
/// answers `Current`, because git ignores the temp directory and reports on
/// the repository the gate is committing to.
#[test]
fn a_directory_outside_any_repository_is_unknown_not_current() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(compare(dir.path(), "c0bcecfc81b157201376553b4e1d7a4d41c4d41f"), Freshness::Unknown);
}

/// The three answers against real history. Guarded rather than asserted
/// unconditionally: built from a source tarball there is no repository here at
/// all, and a test that demanded one would fail for the wrong reason.
#[test]
fn a_checkout_reports_current_stale_by_distance_and_stale_by_absence() {
    let Some(head) = git_in(repo(), &["rev-parse", "HEAD"]) else {
        return; // not a git checkout — nothing to compare against
    };
    assert_eq!(compare(repo(), &head), Freshness::Current);

    // A commit that is not in this repository: the difference is real, the
    // distance is not a number. Forty zeros is a well-formed sha that no
    // repository holds.
    assert_eq!(compare(repo(), &"0".repeat(40)), Freshness::Stale { behind: None });

    // The distance is asserted as a *property*, never as a constant: `HEAD~1` is
    // one step along the first parent, but `rev-list --count HEAD~1..HEAD` counts
    // everything the merge brought in, so on a merge commit an ancestor one step
    // back is legitimately 3 behind. This project lands with `--no-ff` merges, so
    // master's HEAD is routinely a merge and a hardcoded 1 fails on history shape
    // rather than on a real regression — it did, at 37538fe. What must hold is
    // that an ancestor counts, and that a further one counts further.
    if let Some(parent) = git_in(repo(), &["rev-parse", "HEAD~1"]) {
        let Freshness::Stale { behind: Some(near) } = compare(repo(), &parent) else {
            panic!("an ancestor must report a counted distance, not {:?}", compare(repo(), &parent));
        };
        assert!(near >= 1, "an ancestor is at least one commit behind, got {near}");

        if let Some(grandparent) = git_in(repo(), &["rev-parse", "HEAD~2"]) {
            let Freshness::Stale { behind: Some(far) } = compare(repo(), &grandparent) else {
                panic!("an ancestor must report a counted distance");
            };
            assert!(far > near, "HEAD~2 must count further behind than HEAD~1, got {far} vs {near}");
        }
    }
}

/// The probe caches: once an answer arrives it is returned forever, without
/// touching a channel that will never produce another.
#[test]
fn a_ready_probe_answers_immediately_and_keeps_answering() {
    let mut probe = FreshnessProbe::ready(Freshness::Stale { behind: Some(3) });
    assert_eq!(probe.get(), Some(&Freshness::Stale { behind: Some(3) }));
    assert_eq!(probe.get(), Some(&Freshness::Stale { behind: Some(3) }));
}
