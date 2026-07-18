// Chunk-bucketed spatial index for O(visible) range queries over placed entities.

use std::collections::{HashMap, HashSet};

use crate::types::EntityId;

/// Side length (in cells) of one spatial chunk. Entities are bucketed into the
/// chunks their footprint overlaps; range queries only scan the chunks that
/// intersect the query rectangle, so lookup cost scales with the queried area
/// (and its occupants) rather than the total entity count.
pub const CHUNK_SIZE: i32 = 16;

/// Maps chunk coordinates → the ids of entities whose footprint touches that
/// chunk. An entity spanning several chunks is listed in each of them; queries
/// deduplicate before returning.
#[derive(Debug, Default)]
pub struct SpatialIndex {
    chunks: HashMap<(i32, i32), Vec<EntityId>>,
}

/// Floor-divide a cell coordinate into its chunk index. `div_euclid` gives the
/// mathematical floor for the positive `CHUNK_SIZE`, so negative coordinates map
/// to the correct chunk (e.g. cell -1 → chunk -1, not 0).
fn chunk_of(coord: i32) -> i32 {
    coord.div_euclid(CHUNK_SIZE)
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    /// Inclusive chunk range covered by a footprint at `top_left` of `size`.
    fn chunk_range(top_left: (i32, i32), size: (u32, u32)) -> (i32, i32, i32, i32) {
        let (x, y) = top_left;
        let w = size.0 as i32;
        let h = size.1 as i32;
        (
            chunk_of(x),
            chunk_of(y),
            chunk_of(x + w - 1),
            chunk_of(y + h - 1),
        )
    }

    /// Register an entity footprint in every chunk it overlaps.
    pub fn insert(&mut self, id: EntityId, top_left: (i32, i32), size: (u32, u32)) {
        let (min_cx, min_cy, max_cx, max_cy) = Self::chunk_range(top_left, size);
        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                self.chunks.entry((cx, cy)).or_default().push(id);
            }
        }
    }

    /// Remove an entity footprint from every chunk it overlapped. Empty chunk
    /// buckets are dropped so the map does not accumulate dead entries.
    pub fn remove(&mut self, id: EntityId, top_left: (i32, i32), size: (u32, u32)) {
        let (min_cx, min_cy, max_cx, max_cy) = Self::chunk_range(top_left, size);
        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                if let Some(bucket) = self.chunks.get_mut(&(cx, cy)) {
                    bucket.retain(|&e| e != id);
                    if bucket.is_empty() {
                        self.chunks.remove(&(cx, cy));
                    }
                }
            }
        }
    }

    /// Candidate entity ids whose chunk overlaps `[min_x, max_x] × [min_y, max_y]`
    /// (inclusive). This is chunk-coarse: the caller must apply an exact footprint
    /// test. Each id appears at most once even if it spans multiple touched chunks.
    pub fn query_rect(&self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Vec<EntityId> {
        // Tolerate inverted rectangles rather than silently returning nothing.
        let (min_x, max_x) = (min_x.min(max_x), min_x.max(max_x));
        let (min_y, max_y) = (min_y.min(max_y), min_y.max(max_y));

        let min_cx = chunk_of(min_x);
        let min_cy = chunk_of(min_y);
        let max_cx = chunk_of(max_x);
        let max_cy = chunk_of(max_y);

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                if let Some(bucket) = self.chunks.get(&(cx, cy)) {
                    for &id in bucket {
                        if seen.insert(id) {
                            out.push(id);
                        }
                    }
                }
            }
        }
        out
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_of_handles_negatives() {
        assert_eq!(chunk_of(0), 0);
        assert_eq!(chunk_of(15), 0);
        assert_eq!(chunk_of(16), 1);
        assert_eq!(chunk_of(-1), -1);
        assert_eq!(chunk_of(-16), -1);
        assert_eq!(chunk_of(-17), -2);
    }

    #[test]
    fn test_insert_and_query_single() {
        let mut idx = SpatialIndex::new();
        idx.insert(EntityId(0), (0, 0), (1, 1));
        assert_eq!(idx.query_rect(0, 0, 0, 0), vec![EntityId(0)]);
        assert!(idx.query_rect(100, 100, 100, 100).is_empty());
    }

    #[test]
    fn test_multichunk_entity_deduped() {
        // A 20-wide entity straddles chunks 0 and 1 on the x axis.
        let mut idx = SpatialIndex::new();
        idx.insert(EntityId(7), (10, 0), (20, 1));
        // Query spanning both chunks must return the entity exactly once.
        let hits = idx.query_rect(0, 0, 31, 0);
        assert_eq!(hits, vec![EntityId(7)]);
    }

    #[test]
    fn test_remove_clears_entity() {
        let mut idx = SpatialIndex::new();
        idx.insert(EntityId(1), (5, 5), (2, 2));
        idx.remove(EntityId(1), (5, 5), (2, 2));
        assert!(idx.query_rect(5, 5, 6, 6).is_empty());
        assert!(idx.chunks.is_empty(), "empty buckets should be dropped");
    }

    #[test]
    fn test_query_returns_only_overlapping_chunks() {
        let mut idx = SpatialIndex::new();
        idx.insert(EntityId(0), (0, 0), (1, 1)); // chunk (0,0)
        idx.insert(EntityId(1), (100, 100), (1, 1)); // chunk (6,6)
        let hits = idx.query_rect(0, 0, 5, 5);
        assert_eq!(hits, vec![EntityId(0)]);
    }
}
