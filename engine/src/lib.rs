// for example: pub mod renderer; //and then have a renderer.rs
pub mod geom;
pub mod grid;
pub mod level;

use frenderer::{
    input::{Input, Key},
    sprites::{Camera2D, SheetRegion, Transform},
    wgpu, Renderer,
};

#[derive(Debug, PartialEq, Eq)]
pub enum EntityType {
    Player,
    Enemy,
    // which level, x in dest level, y in dest level
    Door(String, u16, u16),
    Gold,
}
#[derive(Clone, Copy, Debug)]
pub struct TileData {
    pub solid: bool,
    pub sheet_region: SheetRegion,
}
const TILE_SZ: usize = 16;