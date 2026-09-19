# In-game checks still outstanding

Things no test replaces, all needing a machine with Factorio on it:

- **Pasting a *filtered* inserter and confirming Factorio honours it.** Idea
  #3377 gives each product of a two-product step its own belt, separated by
  inserter filters. The wire format is taken from `lua-api.factorio.com/2.0.77`
  — the version the registries came from — and pinned key-for-key by a test in
  `crates/grid/src/export.rs`, but nothing here has watched the game parse it.
  The detail to check first is `use_filters`: it defaults to `false`, so if it
  were dropped the inserter would look filtered in the blueprint and grab both
  products in game. Generate a `uranium-processing` block with
  `ingredients_on: Side::Edge` and look at the two centrifuge output inserters.
- **Clipboard watching against the real game** (idea #3348, built 2026-08-13).
  The wiring is tested headlessly end to end — a blueprint offered by the
  watcher loads itself, the app's own copy is declined, a half-typed input
  survives — but every one of those feeds the channel by hand. What no test on
  a machine with no display server can reach is `arboard::Clipboard::new()`
  itself: whether the compositor exposes `wlr-data-control`, and so whether the
  watcher can read the clipboard while *Factorio* holds focus. If it cannot,
  the status bar says so rather than failing silently.
- **Pasting a generated block into Factorio and confirming it runs at rate.**
  Phase 7's blocks pass every unit and integration check, including a
  delivered-rate check measured from the placed grid — but the check that found
  the original half-rate bug was a human watching a belt, and it has not been
  done for the columnar version.
- **Driving the availability UI by hand.** Phase 9's headless egui harness
  exercises the real render path and the reported case end to end, but nobody
  has clicked the tick list in a running window, and panel persistence is
  tested at the serde layer rather than across a real restart.
