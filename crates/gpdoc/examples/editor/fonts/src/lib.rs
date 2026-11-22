pub const FAMILY: &str = "GPDoc Sans";

pub const FACES: &[&[u8]] = &[
    include_bytes!(concat!(env!("OUT_DIR"), "/Inter--400.ttf")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Inter--600.ttf")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Inter--700.ttf")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Inter-Italic-400.ttf")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Inter-Italic-600.ttf")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Inter-Italic-700.ttf")),
];
