// Y-axis row arithmetic for one machine column: where machine `i` starts,
// and the reserved pole rows folded in between groups of them. Split out of
// `place.rs` purely to keep that file to entity placement; `column_height`
// is derived from the same `machine_row_offset` expression `place_cell`
// places from, so sizing and placement can never drift apart.

/// Machines between two consecutive reserved pole rows, derived from the
/// configured pole rather than hardcoded: a 1x1 pole at row `p` covers rows
/// `p - floor(d) ..= p + floor(d)` (`d` = `supply_area_distance`, the supply
/// *half*-width), so poles `2 * floor(d)` rows apart have abutting — not
/// gapped — coverage. Dividing that span by `mh` gives how many machine rows
/// fit between them. `.max(1)` keeps a pole reaching less than one machine's
/// height from producing a zero period and a division by it below.
pub(super) fn pole_period(supply_area_distance: f64, mh: u32) -> u32 {
    (2 * supply_area_distance.floor() as u32 / mh).max(1)
}

/// Row (from a column's own y = 0) where machine `i` (0-based) starts: `mh`
/// rows per machine, plus one reserved, machine-free, inserter-free row
/// before every group of `period` machines — including the very first.
///
/// The reserved row exists because a *vertical* column of poles cannot be
/// inserted anywhere in this cell: every column is load-bearing. An
/// inserter must sit directly beside the machine it serves, and a belt must
/// sit at exactly slot-0 or slot-1 distance from its gutter — nothing can be
/// shifted aside to free a column for a pole without breaking reach. A
/// horizontal row cut straight through the machine columns is the only
/// place `power::place_poles` can ever stand one, so this function reserves
/// it up front rather than leaving placement to hope one turns up later.
pub(super) fn machine_row_offset(i: u32, mh: u32, period: u32) -> i32 {
    (i * mh + 1 + i / period) as i32
}

/// The column height that fits `machines` machines laid out by
/// `machine_row_offset` — computed from that same expression (the last
/// machine's own offset, plus its height) so sizing and placement read from
/// one definition and can never drift apart. Zero machines need zero rows;
/// the caller still applies its own `.max(1)` floor for an entirely empty cell.
pub(super) fn column_height(machines: u32, mh: u32, period: u32) -> u32 {
    match machines.checked_sub(1) {
        Some(last) => (machine_row_offset(last, mh, period) + mh as i32) as u32,
        None => 0,
    }
}
