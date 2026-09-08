#[derive(Clone, Debug, PartialEq)]
pub struct Blueprint {
    pub w: i32,
    pub h: i32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Standard,
    CircleWorld,
    SmallLakes,
    OneSea,
    TwoSeas,
    TwirlingSea,
    NoMansLand,
    ForbiddenCenter,
}

impl Layout {
    pub const ALL: [Layout; 8] = [
        Layout::Standard,
        Layout::CircleWorld,
        Layout::SmallLakes,
        Layout::OneSea,
        Layout::TwoSeas,
        Layout::TwirlingSea,
        Layout::NoMansLand,
        Layout::ForbiddenCenter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Layout::Standard => "Standard",
            Layout::CircleWorld => "Circle World",
            Layout::SmallLakes => "Small Lakes",
            Layout::OneSea => "One Sea",
            Layout::TwoSeas => "Two Seas",
            Layout::TwirlingSea => "Twirling Sea",
            Layout::NoMansLand => "No Mans Land",
            Layout::ForbiddenCenter => "Forbidden Center",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub width: i32,
    pub height: i32,
    pub provinces: i32,
    pub sea_part: i32,
    pub mount_part: i32,
    pub forest_part: i32,
    pub farm_part: i32,
    pub swamp_part: i32,
    pub waste_part: i32,
    pub highland_part: i32,
    pub kelp_part: i32,
    pub gorge_part: i32,
    pub river_part: i32,
    pub hills: i32,
    pub rugedness: i32,
    pub sea_size: i32,
    pub extra_islands: i32,
    pub bridges: i32,
    pub no_water_prov: bool,
    pub hwrap: bool,
    pub vwrap: bool,
    pub blueprint: Option<Blueprint>,
    pub blue_acc: i32,
    pub cave_world: bool,
    pub caves_plane: bool,
    pub cave_part: i32,
    pub cave_blueprint: Option<Blueprint>,
    pub rugedness_f32: Option<f32>,
}

pub const CAVE_RUGEDNESS: i32 = 5;
pub const CAVE_RUGEDNESS_F32: f32 = 0.05;
pub const CAVE_HILLS: i32 = 150;
pub const CAVE_SEA_SIZE: i32 = 100;
pub const CAVE_PROVINCE_CAP: i32 = 0x7c6;
pub const NEWGAME_CAVE_MAP_W: i32 = 2048;
pub const NEWGAME_CAVE_MAP_H: i32 = 1536;
pub const NEWGAME_SMALL_MAX: i32 = 12;
pub const NEWGAME_MEDIUM_MAX: i32 = 17;

pub fn newgame_per_player(per_player_bucket: i32) -> i32 {
    if per_player_bucket <= NEWGAME_SMALL_MAX {
        10
    } else if per_player_bucket <= NEWGAME_MEDIUM_MAX {
        15
    } else {
        20
    }
}

impl Default for Options {
    fn default() -> Self {
        Options {
            width: -1,
            height: -1,
            provinces: 150,
            sea_part: 45,
            mount_part: 20,
            forest_part: 20,
            farm_part: 15,
            swamp_part: 8,
            waste_part: 5,
            highland_part: 10,
            kelp_part: 25,
            gorge_part: 20,
            river_part: 100,
            hills: 150,
            rugedness: 30,
            sea_size: 350,
            extra_islands: 0,
            bridges: 60,
            no_water_prov: false,
            hwrap: true,
            vwrap: false,
            blueprint: None,
            blue_acc: 2,
            cave_world: false,
            caves_plane: false,
            cave_part: 20,
            cave_blueprint: None,
            rugedness_f32: None,
        }
    }
}
impl Options {
    pub fn cave_provinces(&self) -> i32 {
        (2 * self.provinces + 2) / 3
    }

    pub fn caves_plane_options(&self, width: i32, height: i32) -> Options {
        Options {
            width,
            height,
            provinces: self.cave_provinces(),
            sea_part: 100 - self.cave_part,
            rugedness_f32: Some(CAVE_RUGEDNESS_F32),
            mount_part: 0,
            river_part: 0,
            hills: CAVE_HILLS,
            rugedness: CAVE_RUGEDNESS,
            sea_size: CAVE_SEA_SIZE,
            extra_islands: 0,
            blueprint: self.cave_blueprint.clone(),
            cave_blueprint: None,
            cave_world: true,
            caves_plane: false,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caves_plane_options_shrink_and_flood_the_surface_settings() {
        let o = Options {
            provinces: 20,
            caves_plane: true,
            ..Options::default()
        };
        let c = o.caves_plane_options(512, 384);
        assert_eq!(c.provinces, 14);
        assert_eq!((c.width, c.height), (512, 384));
        assert_eq!(c.sea_part, 80);
        assert_eq!(c.mount_part, 0);
        assert_eq!(c.river_part, 0);
        assert_eq!(c.hills, CAVE_HILLS);
        assert_eq!(c.rugedness, CAVE_RUGEDNESS);
        assert_eq!(c.sea_size, CAVE_SEA_SIZE);
        assert_eq!(c.extra_islands, 0);
        assert!(c.cave_world);
        assert!(!c.caves_plane);
        assert_eq!(c.hwrap, o.hwrap);
        assert_eq!(c.blue_acc, o.blue_acc);
    }
}
