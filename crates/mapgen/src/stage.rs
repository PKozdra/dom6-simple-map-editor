#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    NoiseTable,
    Height,
    Seams,
    SeaLevel,
    Capitals,
    Growth,
    Upsample,
    Graph,
    Islands,
    Sizes,
    Rivers,
    Mountains,
    Bridges,
    Cave,
    Paint,
    Margin,
    Edges,
    Recipe,
    Terrain,
    MapText,
    Gates,
}

impl Stage {
    pub const ALL: [Stage; 21] = [
        Stage::NoiseTable,
        Stage::Height,
        Stage::Seams,
        Stage::SeaLevel,
        Stage::Capitals,
        Stage::Growth,
        Stage::Upsample,
        Stage::Graph,
        Stage::Islands,
        Stage::Sizes,
        Stage::Rivers,
        Stage::Mountains,
        Stage::Bridges,
        Stage::Cave,
        Stage::Paint,
        Stage::Margin,
        Stage::Edges,
        Stage::Recipe,
        Stage::Terrain,
        Stage::MapText,
        Stage::Gates,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stage::NoiseTable => "noise_table",
            Stage::Height => "height",
            Stage::Seams => "seams",
            Stage::SeaLevel => "sea_level",
            Stage::Capitals => "capitals",
            Stage::Growth => "growth",
            Stage::Upsample => "upsample",
            Stage::Graph => "graph",
            Stage::Islands => "islands",
            Stage::Sizes => "sizes",
            Stage::Rivers => "rivers",
            Stage::Mountains => "mountains",
            Stage::Bridges => "bridges",
            Stage::Cave => "cave",
            Stage::Paint => "paint",
            Stage::Margin => "margin",
            Stage::Edges => "edges",
            Stage::Recipe => "recipe",
            Stage::Terrain => "terrain",
            Stage::MapText => "map_text",
            Stage::Gates => "gates",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Stage::NoiseTable => "Preparing noise",
            Stage::Height => "Raising the land",
            Stage::Seams => "Blending the edges",
            Stage::SeaLevel => "Filling the seas",
            Stage::Capitals => "Placing capitals",
            Stage::Growth => "Dividing the world",
            Stage::Upsample => "Refining borders",
            Stage::Graph => "Finding neighbours",
            Stage::Islands => "Adding islands",
            Stage::Sizes => "Sizing provinces",
            Stage::Rivers => "Carving rivers",
            Stage::Mountains => "Raising border mountains",
            Stage::Bridges => "Building bridges",
            Stage::Cave => "Digging caves",
            Stage::Paint => "Painting",
            Stage::Margin => "Adding margins",
            Stage::Edges => "Checking exits",
            Stage::Recipe => "Writing the recipe",
            Stage::Terrain => "Rolling terrain",
            Stage::MapText => "Writing the map file",
            Stage::Gates => "Opening cave gates",
        }
    }

    pub fn from_name(s: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|st| st.name() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Continue,
    Cancel,
}

pub trait Sink {
    fn stage(&mut self, stage: Stage, call: u32, hash: u64) -> Control;

    fn wants_hash(&self) -> bool {
        true
    }

    fn progress(&mut self, _stage: Stage, _done: u32, _total: u32) -> Control {
        Control::Continue
    }
}

pub struct NoSink;

impl Sink for NoSink {
    fn stage(&mut self, _stage: Stage, _call: u32, _hash: u64) -> Control {
        Control::Continue
    }
}

#[derive(Default)]
pub struct RecordSink {
    pub entries: Vec<(Stage, u32, u64)>,
}

impl Sink for RecordSink {
    fn stage(&mut self, stage: Stage, call: u32, hash: u64) -> Control {
        self.entries.push((stage, call, hash));
        Control::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for s in Stage::ALL {
            assert_eq!(Stage::from_name(s.name()), Some(s));
        }
    }
}
