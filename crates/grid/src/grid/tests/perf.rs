// The two scaling tests, and the only reason this whole test module has to
// be a child of `grid` rather than an integration test: both reach into
// `entities`, `cells` and `spatial` directly, because measuring the
// containers is the assertion.
use super::*;

// ── Memory footprint sanity test (subtask 6-2) ──────────────────

/// Verify that a grid with 5,000 entities stays within a tight per-entity
/// memory budget — i.e. no accidental per-entity bloat has crept into
/// `PlacedEntity`/`CellState`.
///
/// Per-entity memory breakdown (approximate):
///
/// ```text
/// Option<PlacedEntity> in `entities` vec:
///     PlacedEntity = EntityId(8) + &'static str ptr(8) + GridPos(8) +
///                    Position(16) + Direction(1+pad≈4) + (u32,u32)(8) +
///                    Option<String>(24) + Option<String>(24) ≈ 104 bytes
///     With Option discriminant overhead: ~112 bytes
/// CellState cells in HashMap<(i32,i32), CellState>:
///     Each 1×1 entity occupies 1 cell.
///     HashMap entry ≈ (i32,i32)(8) + CellState(8) + overhead ≈ 40 bytes
/// SpatialIndex HashMap<(i32,i32), Vec<EntityId>> chunk entries:
///     5,000 belts in a ~70×72 area → ceil(70/16)×ceil(72/16) ≈ 20 chunks
///     Each chunk vec entry: 8 bytes per EntityId.
///     Total spatial index ≈ 5000 × 8 = 40 KB (negligible)
///
/// Estimated total for 5,000 1×1 entities:
///   entities vec: 5,000 × 112   =  560 KB
///   cells map:    5,000 × 40    =  200 KB
///   spatial idx:  5,000 × 8     =   40 KB
///   ─────────────────────────────────────
///   Total:                      ≈  800 KB  (< 1 MB for 5,000 entities)
/// ```
///
/// That works out to ~206 B per live entity, comfortably under the
/// 384 B/entity budget the assertion enforces (see below).
#[test]
fn test_memory_footprint_5000_entities() {
    use std::mem::size_of;

    let mut grid = Grid::new();

    // Place 5,000 transport-belt entities in a ~70×72 region.
    // (71 * 71 = 5041 > 5000; we stop at 5000 to hit the target exactly.)
    let mut count = 0_usize;
    'outer: for y in 0..72_i32 {
        for x in 0..71_i32 {
            grid.place(
                "transport-belt",
                &pos(x as f64 + 0.5, y as f64 + 0.5),
                Direction::North,
                None,
                None,
            )
            .unwrap();
            count += 1;
            if count == 5_000 {
                break 'outer;
            }
        }
    }

    assert_eq!(grid.entity_count(), 5_000);

    // Estimate memory consumed by the core data structures.
    //
    // We can't call size_of_val on the Grid itself (it only measures the
    // stack portion, not heap allocations), so we compute an upper-bound
    // estimate from known element sizes × counts.

    // entities vec: each slot is Option<PlacedEntity>
    let entity_slot_bytes = size_of::<Option<PlacedEntity>>();
    let entities_heap = entity_slot_bytes * grid.entities.capacity();

    // cells hashmap: each entry holds a (i32,i32) key and CellState value.
    // HashMap has ~1.8× load overhead, so we multiply by 2 to be safe.
    let cell_entry_bytes = size_of::<(i32, i32)>() + size_of::<CellState>();
    let cells_heap = cell_entry_bytes * grid.cells.capacity();

    // Per-entity String heap for recipe/entity_type (None for belts → 0).
    // Include it for correctness even though it's zero here.
    let strings_heap: usize = grid
        .entities()
        .map(|e| {
            e.recipe.as_ref().map_or(0, |s| s.capacity())
                + e.entity_type.as_ref().map_or(0, |s| s.capacity())
        })
        .sum();

    let total_bytes = entities_heap + cells_heap + strings_heap;

    // Tight per-entity budget rather than an unfalsifiable absolute ceiling.
    // The theoretical minimum is ~112 B for the `Option<PlacedEntity>` slot
    // plus ~16 B for its cell entry ≈ 128 B/live-entity; container
    // over-allocation (Vec capacity doubling, HashMap load factor) roughly
    // doubles the effective figure. A 384 B/entity ceiling absorbs that
    // over-allocation while still tripping on any real regression — a
    // doubling of `PlacedEntity`'s size, or an added kilobyte-scale field,
    // blows straight past it (unlike the old 500 MB / ~500× headroom bound,
    // which no plausible implementation could ever fail).
    const MAX_BYTES_PER_ENTITY: usize = 384;
    let ceiling = MAX_BYTES_PER_ENTITY * grid.entity_count();
    let bytes_per_entity = total_bytes as f64 / grid.entity_count() as f64;
    assert!(
        total_bytes < ceiling,
        "estimated {bytes_per_entity:.1} B/entity exceeds the \
         {MAX_BYTES_PER_ENTITY} B/entity ceiling for {} entities",
        grid.entity_count()
    );
}

// ── Performance test ─────────────────────────────────────────────

/// Verify that `query_rect` on a large grid scales with the queried area,
/// not total entity count.  We place 10,000 transport-belt entities (1×1) in
/// a 100×100 grid, then query a 10×10 region and assert:
///   1. The returned entity count matches the expected 100 entities in
///      that region.
///   2. The spatial index scans only the single chunk the query touches
///      (256 candidate occupants), not all 10,000 entities — demonstrating
///      O(touched-chunks) rather than O(all-entities) behaviour.
///
/// The scaling property is checked structurally (candidate count) rather
/// than with a wall-clock budget, which would be flaky under load, coverage
/// instrumentation, or debug builds.
#[test]
fn test_query_rect_performance_10k_entities() {
    let mut grid = Grid::new();

    // Place 10,000 transport-belt entities at (x+0.5, y+0.5) for x,y in 0..100
    for y in 0..100_i32 {
        for x in 0..100_i32 {
            grid.place(
                "transport-belt",
                &pos(x as f64 + 0.5, y as f64 + 0.5),
                Direction::North,
                None,
                None,
            )
            .unwrap();
        }
    }

    assert_eq!(grid.entity_count(), 10_000);

    // Query a 10×10 region (cells 0..=9 in both axes → 100 entities)
    let results = grid.query_rect(0, 0, 9, 9);

    assert_eq!(
        results.len(),
        100,
        "expected exactly 100 entities in the 10×10 query region"
    );

    // Structural proof of O(touched-chunks), not O(all-entities): the query
    // region 0..=9 lies entirely within chunk (0,0) (16×16 cells), so the
    // spatial index scans only that one chunk's bucket — the 256 belts in
    // cells 0..15 × 0..15 — never the full 10,000. The exact-AABB filter
    // above then narrows those 256 candidates to the 100 in-region hits.
    let candidates = grid.spatial.query_rect(0, 0, 9, 9);
    assert_eq!(
        candidates.len(),
        256,
        "spatial index should scan only the single touched chunk's 256 \
         occupants, independent of the 10,000 total entities"
    );
}
