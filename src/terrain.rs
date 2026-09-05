pub const SMALL: u64 = 1 << 0;
pub const LARGE: u64 = 1 << 1;
pub const SEA: u64 = 1 << 2;
pub const FRESH_WATER: u64 = 1 << 3;
pub const HIGHLAND: u64 = 1 << 4;
pub const SWAMP: u64 = 1 << 5;
pub const WASTE: u64 = 1 << 6;
pub const FOREST: u64 = 1 << 7;
pub const FARM: u64 = 1 << 8;
pub const NO_START: u64 = 1 << 9;
pub const MANY_SITES: u64 = 1 << 10;
pub const DEEP_SEA: u64 = 1 << 11;
pub const CAVE: u64 = 1 << 12;
pub const MOUNTAIN: u64 = 1 << 23;
pub const GOOD_THRONE: u64 = 1 << 25;
pub const GOOD_START: u64 = 1 << 26;
pub const BAD_THRONE: u64 = 1 << 27;
pub const WARMER: u64 = 1 << 30;
pub const COLDER: u64 = 1 << 31;
pub const UNKNOWN: u64 = 1 << 32;
pub const BORDER_MOUNTAIN: u64 = 1 << 34;
pub const CAVE_WALL: u64 = 1 << 36;
pub const GATEWAY: u64 = 1 << 37;
pub const CAVE_LOOK: u64 = 1 << 59;
pub const ALWAYS_WATER: u64 = 1 << 60;
pub const KELP_EXACT: u64 = 0x84;

pub const NAMED: &[(u64, &str)] = &[
    (SMALL, "Small province"),
    (LARGE, "Large province"),
    (SEA, "Sea"),
    (FRESH_WATER, "Fresh water"),
    (HIGHLAND, "Highlands"),
    (SWAMP, "Swamp"),
    (WASTE, "Waste"),
    (FOREST, "Forest"),
    (FARM, "Farm"),
    (NO_START, "No start"),
    (MANY_SITES, "Many sites"),
    (DEEP_SEA, "Deep sea"),
    (CAVE, "Cave"),
    (MOUNTAIN, "Mountain"),
    (GOOD_THRONE, "Throne site"),
    (GOOD_START, "Start"),
    (BAD_THRONE, "No throne"),
    (WARMER, "Warmer"),
    (COLDER, "Colder"),
    (UNKNOWN, "Unknown"),
    (BORDER_MOUNTAIN, "Border mountain"),
    (CAVE_WALL, "Cave wall"),
    (GATEWAY, "Gateway"),
    (CAVE_LOOK, "Cave look"),
    (ALWAYS_WATER, "Always water"),
];

pub fn describe(mask: u64) -> String {
    let mut parts: Vec<&str> = NAMED
        .iter()
        .filter(|(b, _)| mask & b != 0)
        .map(|(_, n)| *n)
        .collect();
    if parts.is_empty() {
        parts.push("Plains");
    }
    parts.join(", ")
}

pub fn is_water(mask: u64) -> bool {
    mask & SEA != 0
}

pub fn make_sea(mask: u64, deep: bool) -> u64 {
    let mut m = (mask | SEA) & !FRESH_WATER;
    if deep {
        m |= DEEP_SEA;
    } else {
        m &= !DEEP_SEA;
    }
    m
}

pub fn make_land(mask: u64) -> u64 {
    mask & !(SEA | DEEP_SEA)
}

pub const BORDER_MOUNTAIN_PASS: u64 = 1;
pub const BORDER_RIVER: u64 = 2;
pub const BORDER_IMPASSABLE: u64 = 4;
pub const BORDER_BRIDGE: u64 = 0x10;
pub const BORDER_MOUNTAIN_LINE: u64 = 0x20;
pub const BORDER_CARVED: u64 = BORDER_RIVER | BORDER_BRIDGE;
