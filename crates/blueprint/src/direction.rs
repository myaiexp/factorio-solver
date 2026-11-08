// Direction: the 16-way Factorio 2.0 enum, its u8 serde, and legacy 1.x direction detection.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ── Direction ─────────────────────────────────────────────────────────

/// Factorio 2.0 uses 16 directions (0–15). Cardinal directions are at
/// multiples of 4: North=0, East=4, South=8, West=12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    #[default]
    North = 0,
    NorthNorthEast = 1,
    NorthEast = 2,
    EastNorthEast = 3,
    East = 4,
    EastSouthEast = 5,
    SouthEast = 6,
    SouthSouthEast = 7,
    South = 8,
    SouthSouthWest = 9,
    SouthWest = 10,
    WestSouthWest = 11,
    West = 12,
    WestNorthWest = 13,
    NorthWest = 14,
    NorthNorthWest = 15,
}

impl Direction {
    /// Raw Factorio direction byte (0–15 in the 2.0 scheme).
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Direction> {
        match v {
            0 => Some(Direction::North),
            1 => Some(Direction::NorthNorthEast),
            2 => Some(Direction::NorthEast),
            3 => Some(Direction::EastNorthEast),
            4 => Some(Direction::East),
            5 => Some(Direction::EastSouthEast),
            6 => Some(Direction::SouthEast),
            7 => Some(Direction::SouthSouthEast),
            8 => Some(Direction::South),
            9 => Some(Direction::SouthSouthWest),
            10 => Some(Direction::SouthWest),
            11 => Some(Direction::WestSouthWest),
            12 => Some(Direction::West),
            13 => Some(Direction::WestNorthWest),
            14 => Some(Direction::NorthWest),
            15 => Some(Direction::NorthNorthWest),
            _ => None,
        }
    }

    /// Map a Factorio 1.x direction byte (0–7 scheme) into the 2.0 16-way value.
    ///
    /// 1.x N/E/S/W = 0/2/4/6 become 2.0 0/4/8/12 (`value * 2`).
    pub fn from_legacy_u8(v: u8) -> Option<Direction> {
        if v <= 7 {
            Direction::from_u8(v * 2)
        } else {
            None
        }
    }

    /// Upgrade a direction that was decoded under the 2.0 table but originated
    /// as a 1.x cardinal encoding (raw 0/2/4/6 → 2.0 N/E/S/W).
    pub fn upgrade_from_legacy(self) -> Direction {
        Direction::from_legacy_u8(self.as_u8()).unwrap_or(self)
    }
}

/// Factorio packs `version` as `major<<48 | minor<<32 | patch<<16 | dev`.
pub fn factorio_major_version(version: u64) -> u16 {
    (version >> 48) as u16
}

/// Detect Factorio 1.x cardinal direction encoding in a set of decoded values.
///
/// 1.x uses N/E/S/W = 0/2/4/6. After a naive 2.0 decode those become
/// North/NorthEast/East/SouthEast. A pure 2.0 blueprint that only faces
/// North+East is {0, 4} and must **not** be rewritten (4 would become South).
///
/// Rules:
/// - Any direction outside `{0,2,4,6}` ⇒ not pure 1.x encoding (modern 2.0).
/// - Blueprint major version `< 2` ⇒ treat any pure `{0,2,4,6}` set as legacy
///   (covers pure-South raw 4 and North+South-only sets that lack East/West markers).
/// - Blueprint major version `≥ 2` ⇒ only upgrade when a definitive 1.x marker
///   (decoded 2 or 6) is present; ambiguous `{0,4}` stays as 2.0 North+East.
pub fn directions_look_legacy(
    dirs: impl IntoIterator<Item = Direction>,
    version: u64,
) -> bool {
    let mut saw_marker = false;
    let mut any = false;
    for d in dirs {
        any = true;
        match d.as_u8() {
            0 | 4 => {}
            2 | 6 => saw_marker = true,
            _ => return false,
        }
    }
    if !any {
        return false;
    }
    if factorio_major_version(version) < 2 {
        // 1.x always used the 0/2/4/6 cardinal scheme.
        return true;
    }
    // 2.0+: require a definitive East/West marker so pure North+East is kept.
    saw_marker
}

impl Serialize for Direction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Direction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        Direction::from_u8(v).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid direction value: {v}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Direction serde tests

    #[test]
    fn test_direction_serializes_as_u8() {
        let val = serde_json::to_value(Direction::East).unwrap();
        assert_eq!(val, json!(4));
    }

    #[test]
    fn test_direction_deserializes_from_u8() {
        let dir: Direction = serde_json::from_value(json!(8)).unwrap();
        assert_eq!(dir, Direction::South);
    }

    #[test]
    fn test_direction_default_is_north() {
        assert_eq!(Direction::default(), Direction::North);
    }

    #[test]
    fn test_direction_zero_deserializes() {
        let dir: Direction = serde_json::from_value(json!(0)).unwrap();
        assert_eq!(dir, Direction::North);
    }

    #[test]
    fn test_direction_invalid_value_errors() {
        let result = serde_json::from_value::<Direction>(json!(16));
        assert!(result.is_err());

        let result = serde_json::from_value::<Direction>(json!(255));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_legacy_u8_maps_1x_cardinals() {
        assert_eq!(Direction::from_legacy_u8(0), Some(Direction::North));
        assert_eq!(Direction::from_legacy_u8(2), Some(Direction::East));
        assert_eq!(Direction::from_legacy_u8(4), Some(Direction::South));
        assert_eq!(Direction::from_legacy_u8(6), Some(Direction::West));
        assert!(Direction::from_legacy_u8(8).is_none());
    }

    #[test]
    fn test_upgrade_from_legacy_after_naive_2_0_decode() {
        // Codec maps 1.x bytes onto 2.0 variants by raw value; upgrade fixes them.
        assert_eq!(Direction::NorthEast.upgrade_from_legacy(), Direction::East); // 2
        assert_eq!(Direction::East.upgrade_from_legacy(), Direction::South); // 4
        assert_eq!(Direction::SouthEast.upgrade_from_legacy(), Direction::West); // 6
        assert_eq!(Direction::North.upgrade_from_legacy(), Direction::North); // 0
    }

    const VERSION_1_1: u64 = 281479275675648; // major 1
    const VERSION_2_0: u64 = 2u64 << 48;

    #[test]
    fn test_factorio_major_version() {
        assert_eq!(factorio_major_version(VERSION_1_1), 1);
        assert_eq!(factorio_major_version(VERSION_2_0), 2);
        assert_eq!(factorio_major_version(0), 0);
    }

    #[test]
    fn test_directions_look_legacy_version_and_markers() {
        // Definitive 1.x East/West markers — legacy under both majors.
        assert!(directions_look_legacy(
            [Direction::North, Direction::NorthEast],
            VERSION_2_0
        ));
        assert!(directions_look_legacy([Direction::SouthEast], VERSION_2_0));
        assert!(directions_look_legacy([Direction::SouthEast], VERSION_1_1));

        // Ambiguous pure {0,4}: 2.0 keeps as North+East; 1.x upgrades (4→South).
        assert!(!directions_look_legacy(
            [Direction::North, Direction::East],
            VERSION_2_0
        ));
        assert!(directions_look_legacy(
            [Direction::North, Direction::East],
            VERSION_1_1
        ));

        // Pure South (raw 4 only): same disambiguation.
        assert!(!directions_look_legacy([Direction::East], VERSION_2_0));
        assert!(directions_look_legacy([Direction::East], VERSION_1_1));

        // Any true 2.0-only value rejects legacy mode regardless of version.
        assert!(!directions_look_legacy(
            [Direction::NorthEast, Direction::South],
            VERSION_1_1
        ));
        assert!(!directions_look_legacy(
            [Direction::NorthEast, Direction::South],
            VERSION_2_0
        ));
        assert!(!directions_look_legacy(std::iter::empty(), VERSION_1_1));
    }
}
