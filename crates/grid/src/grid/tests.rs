// Test-module root for `Grid`: the shared `pos` helper, and the split.
//
// A child module rather than `crates/grid/tests/`, because several of these
// read private fields (`entities`, `cells`, `spatial`) that an integration
// test cannot reach — the memory-footprint and query_rect-scaling tests
// measure the containers directly, which is the whole point of them.
use super::*;

mod bbox_cache;
mod bounds;
mod filters;
mod perf;
mod placement;
mod query;

/// Helper to make positions concise.
fn pos(x: f64, y: f64) -> Position {
    Position { x, y }
}
