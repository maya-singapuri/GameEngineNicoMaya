use assets_manager::{asset::Png, AssetCache};
use frenderer::{
    input::{Input, Key},
    sprites::{Camera2D, SheetRegion, Transform},
    wgpu, Renderer,
};
use engine::geom::*;
use engine::level;
use engine::grid;
use engine::EntityType;
use engine::level::Level;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use rand::seq::SliceRandom;
use rand::Rng;

#[derive(Clone, Copy)]
struct Entity {
    pos: Vec2,
    dir: Vec2,
    #[allow(dead_code)]
    pattern: MovementPattern,
}

