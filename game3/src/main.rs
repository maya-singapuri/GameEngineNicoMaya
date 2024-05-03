use assets_manager::{asset::Png, AssetCache};
use frenderer::{
    input::{Input, Key},
    sprites::{Camera2D, SheetRegion, Transform},
    wgpu, Renderer,
};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;
// mod geom3;
mod grid3;
// use geom3::*;
use engine::geom::*;
mod level3;
use level3::{EntityType, Level};

const GRAVITY: f32 = -15.0;
const GRAV_ACC: f32 = 300.0;
const WALK_ACC: f32 = 180.0;
const MAX_SPEED: f32 = 90.0;
const BRAKE_DAMP: f32 = 0.5;
const JUMP_VEL: f32 = 140.0;
const JUMP_TIME_MAX: f32 = 0.15;
const ATTACK_MAX_TIME: f32 = 0.6;
const ATTACK_COOLDOWN_TIME: f32 = 0.1;
const SPAWN_INTERVAL: f32 = 0.075;
const SPAWN_HEIGHT: f32 = 200.0;
const FALLING_VELOCITY: f32 = -100.0;
const MIN_SPAWN_DISTANCE: f32 = 50.0;
const HEART: SheetRegion = SheetRegion::rect(305, 281, 8, 8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    E,
    W,
}

impl Dir {
    fn to_vec2(self) -> Vec2 {
        Vec2 {
            x: match self {
                Dir::E => 1.0,
                Dir::W => -1.0,
            },
            y: 0.0,
        }
    }
}
struct Game {
    current_level: usize,
    levels: Vec<Level>,
    player: Player,
    enemies: Vec<Enemy>,
    doors: Vec<(String, (u16, u16), Vec2)>,
    camera: Camera2D,
    animations: Vec<Animation>,
    spawn_timer: f32,
    last_spawn_x: f32,
    health: usize,
    game_over: bool,
    game_start_time: std::time::Instant,
}

struct Enemy {
    pos: Vec2,
    dir: Dir,
    vel: Vec2,
    dead: bool,
}
impl Enemy {
    fn die(&mut self) {
        self.dead = true;
    }
    fn rect(&self) -> Rect {
        if self.dead {
            return Rect::ZERO;
        }
        Rect {
            x: self.pos.x - 18.0 + 6.0,
            y: self.pos.y - 8.0,
            w: 24,
            h: 8,
        }
    }
    fn collides_with(&self, player: &Player) -> bool {
        !self.dead && self.rect().overlap(player.rect()).is_some()
    }
    fn trf(&self) -> Transform {
        if self.dead {
            return Transform::ZERO;
        }
        Transform {
            w: 36 / 2,
            h: 16 / 2,
            x: self.pos.x,
            y: self.pos.y,
            rot: 0.0,
        }
    }
}

struct Player {
    pos: Vec2, // player, entities, other dynamic info here
    vel: Vec2,
    dir: Dir,
    touching_door: bool,
    anim: AnimationState,
    shrunk: bool,
    shrink_timer: f32,
}
impl Player {
    fn rect(&self) -> Rect {
        Rect {
            x: self.pos.x
                + match self.dir {
                    Dir::E => 8.0,
                    Dir::W => 12.0,
                },
            y: self.pos.y - 12.0,
            w: 16,
            h: 24,
        }
    }
    fn trf(&self, game_over: bool) -> Transform {
        let shrink_factor = if self.shrunk { 0.5 } else { 1.0 };
        if game_over {
            Transform {
                w: (36.0 * shrink_factor) as u16,
                h: (36.0 * shrink_factor) as u16,
                x: self.pos.x + 2.0,
                y: self.pos.y - 1.0,
                rot: 91.0,
            }
        } else {
            Transform {
                w: (36.0 * shrink_factor) as u16,
                h: (36.0 * shrink_factor) as u16,
                x: self.pos.x + (self.dir.to_vec2().x * 8.0) + 18.0 * shrink_factor,
                y: self.pos.y - 12.0 + 18.0 * shrink_factor,
                rot: 0.0,
            }
        }
    }
    // fn attack_rect(&self) -> Rect {
    //     Rect {
    //         x: self.pos.x
    //             + match self.dir {
    //                 Dir::E => 16.0,
    //                 Dir::W => 0.0,
    //             },
    //         y: self.pos.y - 12.0,
    //         w: if self.attack_timer > ATTACK_COOLDOWN_TIME {
    //             18
    //         } else {
    //             0
    //         },
    //         h: if self.attack_timer > ATTACK_COOLDOWN_TIME {
    //             24
    //         } else {
    //             0
    //         },
    //     }
    // }
}

mod animation3;
use animation3::Animation;

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum AnimationKey {
    Blank = 0,
    PlayerRightIdle,
    PlayerRightWalk,
    PlayerRightJumpRise,
    PlayerRightJumpFall,
    PlayerRightAttack,
    PlayerLeftIdle,
    PlayerLeftWalk,
    PlayerLeftJumpRise,
    PlayerLeftJumpFall,
    PlayerLeftAttack,
    EnemyRightWalk,
    EnemyLeftWalk,
}
struct AnimationState {
    animation: AnimationKey,
    t: f32,
}
#[allow(unused)]
impl AnimationState {
    fn finished(&self, anims: &[Animation]) -> bool {
        anims[self.animation as usize].sample(self.t).is_none()
    }
    fn sample(&self, anims: &[Animation]) -> SheetRegion {
        anims[self.animation as usize]
            .sample(self.t)
            .unwrap_or_else(|| anims[self.animation as usize].sample(0.0).unwrap())
    }
    fn tick(&mut self, dt: f32, game_over: bool) {
        if !game_over {
            self.t += dt;
        }
    }
    fn play(&mut self, anim: AnimationKey, retrigger: bool) {
        if anim == self.animation && !retrigger {
            return;
        }
        self.animation = anim;
        self.t = 0.0;
    }
}
// Feel free to change this if you use a different tilesheet
const TILE_SZ: usize = 16;
const W: usize = 16 * TILE_SZ;
const H: usize = 12 * TILE_SZ;
const SCREEN_FAST_MARGIN: f32 = 64.0;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let source =
        assets_manager::source::FileSystem::new("content").expect("Couldn't load resources");
    #[cfg(target_arch = "wasm32")]
    let source = assets_manager::source::Embedded::from(assets_manager::source::embed!("content"));
    let cache = assets_manager::AssetCache::with_source(source);

    let drv = frenderer::Driver::new(
        winit::window::WindowBuilder::new()
            .with_title("test")
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 768.0)),
        Some((W as u32, H as u32)),
    );

    const DT: f32 = 1.0 / 60.0;
    let mut input = Input::default();

    let mut now = frenderer::clock::Instant::now();
    let mut acc = 0.0;
    drv.run_event_loop::<(), _>(
        move |window, mut frend| {
            let game = Game::new(&mut frend, &cache);
            (window, game, frend)
        },
        move |event, target, (window, ref mut game, ref mut frend)| {
            use winit::event::{Event, WindowEvent};
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    target.exit();
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    if !frend.gpu.is_web() {
                        frend.resize_surface(size.width, size.height);
                    }
                    window.request_redraw();
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let elapsed = now.elapsed().as_secs_f32();
                    // You can add the time snapping/death spiral prevention stuff here if you want.
                    // I'm not using it here to keep the starter code small.
                    acc += elapsed;
                    now = std::time::Instant::now();
                    // While we have time to spend
                    while acc >= DT {
                        // simulate a frame
                        acc -= DT;
                        game.simulate(&input, DT);
                        input.next_frame();
                    }
                    game.render(frend);
                    frend.render();
                    window.request_redraw();
                }
                event => {
                    input.process_input_event(&event);
                }
            }
        },
    )
    .expect("event loop error");
}

impl Game {
    fn new(renderer: &mut Renderer, cache: &AssetCache) -> Self {
        let tile_handle = cache
            .load::<Png>("tileset")
            .expect("Couldn't load tilesheet img");
        let tile_img = tile_handle.read().0.to_rgba8();
        let tile_tex = renderer.create_array_texture(
            &[&tile_img],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            tile_img.dimensions(),
            Some("tiles-sprites"),
        );
        let levels = vec![
            Level::from_str(
                &cache
                    .load::<String>("level1")
                    .expect("Couldn't access level1.txt")
                    .read(),
            ),
            Level::from_str(
                &cache
                    .load::<String>("house")
                    .expect("Couldn't access house.txt")
                    .read(),
            ),
            Level::from_str(
                &cache
                    .load::<String>("shop")
                    .expect("Couldn't access shop.txt")
                    .read(),
            ),
        ];
        let current_level = 0;
        let camera = Camera2D {
            screen_pos: [0.0, 0.0],
            screen_size: [W as f32, H as f32],
        };
        let sprite_estimate =
            levels[current_level].sprite_count() + levels[current_level].starts().len();
        renderer.sprite_group_add(
            &tile_tex,
            vec![Transform::ZERO; sprite_estimate],
            vec![SheetRegion::ZERO; sprite_estimate],
            camera,
        );
        let player_start = levels[current_level]
            .starts()
            .iter()
            .find(|(t, _)| *t == EntityType::Player)
            .map(|(_, ploc)| *ploc + Vec2 { x: 0.0, y: -12.0 })
            .expect("Start level doesn't put the player anywhere");

        let level_width = levels[current_level].width() as f32 * TILE_SZ as f32;
        let mut game = Game {
            game_start_time: std::time::Instant::now(),
            current_level,
            camera,
            levels,
            last_spawn_x: rand::random::<f32>() * level_width,
            spawn_timer: SPAWN_INTERVAL,
            enemies: vec![],
            doors: vec![],
            health: 5,
            game_over: false,
            player: Player {
                vel: Vec2 { x: 0.0, y: 0.0 },
                pos: player_start,
                dir: Dir::E,
                anim: AnimationState {
                    animation: AnimationKey::PlayerRightIdle,
                    t: 0.0,
                },
                touching_door: false,
                shrunk: false,
                shrink_timer: 0.0,
            },
            animations: vec![
                Animation::with_frame(SheetRegion::ZERO),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(0, 300, 36, 36),
                        SheetRegion::rect(60, 300, 36, 36),
                        SheetRegion::rect(120, 300, 36, 36),
                        SheetRegion::rect(180, 300, 36, 36),
                        SheetRegion::rect(300, 300, 36, 36),
                    ],
                    0.15,
                )
                .looped(),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(300, 300, 36, 36),
                        SheetRegion::rect(360, 300, 36, 36),
                        SheetRegion::rect(420, 300, 36, 36),
                    ],
                    0.15,
                )
                .looped(),
                Animation::with_frame(SheetRegion::rect(480, 300, 36, 36)),
                Animation::with_frame(SheetRegion::rect(539, 300, 36, 36)),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(779, 300, 36, 36),
                        SheetRegion::rect(839, 300, 36, 36),
                        SheetRegion::rect(899, 300, 36, 36),
                        SheetRegion::rect(959, 300, 36, 36),
                        SheetRegion::rect(1019, 300, 36, 36),
                        SheetRegion::rect(1079, 300, 36, 36),
                        SheetRegion::rect(1139, 300, 36, 36),
                    ],
                    0.07,
                ),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(0, 300, 36, 36),
                        SheetRegion::rect(60, 300, 36, 36),
                        SheetRegion::rect(120, 300, 36, 36),
                        SheetRegion::rect(180, 300, 36, 36),
                        SheetRegion::rect(300, 300, 36, 36),
                    ],
                    0.15,
                )
                .looped()
                .flip_horizontal(),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(300, 300, 36, 36),
                        SheetRegion::rect(360, 300, 36, 36),
                        SheetRegion::rect(420, 300, 36, 36),
                    ],
                    0.15,
                )
                .looped()
                .flip_horizontal(),
                Animation::with_frame(SheetRegion::rect(480, 300, 36, 36)).flip_horizontal(),
                Animation::with_frame(SheetRegion::rect(539, 300, 36, 36)).flip_horizontal(),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(779, 300, 36, 36),
                        SheetRegion::rect(839, 300, 36, 36),
                        SheetRegion::rect(899, 300, 36, 36),
                        SheetRegion::rect(959, 300, 36, 36),
                        SheetRegion::rect(1019, 300, 36, 36),
                        SheetRegion::rect(1079, 300, 36, 36),
                        SheetRegion::rect(1139, 300, 36, 36),
                    ],
                    0.07,
                )
                .flip_horizontal(),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(0, 272, 36, 16),
                        SheetRegion::rect(36, 272, 36, 16),
                        SheetRegion::rect(36 * 2, 272, 36, 16),
                        SheetRegion::rect(36 * 3, 272, 36, 16),
                        SheetRegion::rect(36 * 4, 272, 36, 16),
                        SheetRegion::rect(36 * 5, 272, 36, 16),
                        SheetRegion::rect(36 * 6, 272, 36, 16),
                        SheetRegion::rect(36 * 7, 272, 36, 16),
                        //SheetRegion::rect(36 * 8, 272, 36, 16),
                    ],
                    0.1,
                )
                .looped(),
                Animation::with_frames(
                    &[
                        SheetRegion::rect(0, 272, 36, 16),
                        SheetRegion::rect(36, 272, 36, 16),
                        SheetRegion::rect(36 * 2, 272, 36, 16),
                        SheetRegion::rect(36 * 3, 272, 36, 16),
                        SheetRegion::rect(36 * 4, 272, 36, 16),
                        SheetRegion::rect(36 * 5, 272, 36, 16),
                        SheetRegion::rect(36 * 6, 272, 36, 16),
                        SheetRegion::rect(36 * 7, 272, 36, 16),
                        //SheetRegion::rect(36 * 8, 272, 36, 16),
                    ],
                    0.1,
                )
                .looped()
                .flip_horizontal(),
            ],
        };
        game.enter_level(player_start);
        game
    }
    fn level(&self) -> &Level {
        &self.levels[self.current_level]
    }
    fn enter_level(&mut self, player_pos: Vec2) {
        self.doors.clear();
        self.enemies.clear();
        // we will probably enter at a door
        self.player.touching_door = true;
        self.player.pos = player_pos;
        for (etype, pos) in self.levels[self.current_level].starts().iter() {
            match etype {
                EntityType::Player => {}
                EntityType::Door(rm, x, y) => self.doors.push((rm.clone(), (*x, *y), *pos)),
                EntityType::Enemy => self.enemies.push(Enemy {
                    dead: false,
                    pos: Vec2 { x: pos.x, y: 200.0 },
                    vel: Vec2 { x: 0.0, y: 0.0 },
                    dir: Dir::W,
                }),
            }
        }
    }

    fn spawn_enemy(&mut self) {
        let level_width = self.level().width() as f32 * TILE_SZ as f32;

        let mut new_x_position =
            self.last_spawn_x + MIN_SPAWN_DISTANCE + rand::random::<f32>() * 100.0; // Adding some random value

        if new_x_position > level_width {
            new_x_position -= level_width;
        }

        self.enemies.push(Enemy {
            dead: false,
            pos: Vec2 {
                x: new_x_position,
                y: SPAWN_HEIGHT,
            },
            vel: Vec2 {
                x: 0.0,
                y: FALLING_VELOCITY,
            },
            dir: Dir::W,
        });

        self.last_spawn_x = new_x_position;
    }
    fn sprite_count(&self) -> usize {
        //todo!("count how many entities and other sprites we have");
        self.level().sprite_count() + self.enemies.len() + self.health + 1
    }

    fn render(&mut self, frend: &mut Renderer) {
        // make this exactly as big as we need
        frend.sprite_group_resize(0, self.sprite_count());
        frend.sprite_group_set_camera(0, self.camera);

        let sprites_used = self.level().render_into(frend, 0);
        let (sprite_posns, sprite_gfx) = frend.sprites_mut(0, sprites_used..);

        for (enemy, (trf, uv)) in self
            .enemies
            .iter()
            .zip(sprite_posns.iter_mut().zip(sprite_gfx.iter_mut()))
        {
            *trf = enemy.trf();
            //*uv = enemy.anim.sample(&self.animations);
            *uv = self.animations[AnimationKey::EnemyLeftWalk as usize]
                .sample(0.0)
                .unwrap();
        }
        let sprite_posns = &mut sprite_posns[self.enemies.len()..];
        let sprite_gfx = &mut sprite_gfx[self.enemies.len()..];
        sprite_posns[0] = self.player.trf(self.game_over);
        sprite_gfx[0] = self.player.anim.sample(&self.animations);

        let start_x = 20.0
            + (self.camera.screen_size[0] - self.health as f32 * (HEART.w as f32 + 10.0)) / 2.0;
        for i in 0..self.health {
            let heart_transform = Transform {
                x: start_x + i as f32 * (HEART.w as f32 + 2.5),
                y: 180.0,
                w: HEART.w as u16,
                h: HEART.h as u16,
                rot: 0.0,
            };
            sprite_posns[i + 1] = heart_transform;
            sprite_gfx[i + 1] = HEART;
        }
    }

    fn simulate(&mut self, input: &Input, dt: f32) {
        if self.game_over {
            self.end_game();
        } else {
            self.player.vel.x = MAX_SPEED * input.key_axis(Key::ArrowLeft, Key::ArrowRight);
            self.player.pos += self.player.vel * dt;

            if self.player.vel.x > 0.0 {
                self.player.dir = Dir::E;
            } else if self.player.vel.x < 0.0 {
                self.player.dir = Dir::W;
            }
            if self.player.vel.x.abs() > 0.1 {
                let walk_anim = match self.player.dir {
                    Dir::E => AnimationKey::PlayerRightWalk,
                    Dir::W => AnimationKey::PlayerLeftWalk,
                };
                self.player.anim.play(walk_anim, false);
            } else {
                let idle_anim = match self.player.dir {
                    Dir::E => AnimationKey::PlayerRightIdle,
                    Dir::W => AnimationKey::PlayerLeftIdle,
                };
                self.player.anim.play(idle_anim, false);
            }

            if self.player.vel.y > 0.0 {
                let jump_anim = match self.player.dir {
                    Dir::E => AnimationKey::PlayerRightJumpRise,
                    Dir::W => AnimationKey::PlayerLeftJumpRise,
                };
                self.player.anim.play(jump_anim, false);
            } else if self.player.vel.y < 0.0 {
                let fall_anim = match self.player.dir {
                    Dir::E => AnimationKey::PlayerRightJumpFall,
                    Dir::W => AnimationKey::PlayerLeftJumpFall,
                };
                self.player.anim.play(fall_anim, false);
            }

            self.player.anim.tick(dt, self.game_over);
        }

        let lw = self.level().width();
        let lh = self.level().height();
        self.player.pos.x = self.player.pos.x.clamp(
            0.0,
            lw as f32 * TILE_SZ as f32 - self.player.rect().w as f32 - 130.0,
        );
        self.player.pos.y = self.player.pos.y.clamp(
            0.0,
            lh as f32 * TILE_SZ as f32 * H as f32 - self.player.rect().h as f32 / 2.0,
        );
        self.player.anim.tick(dt, self.game_over);

        if input.is_key_pressed(Key::Space) && !self.player.shrunk {
            self.player.shrunk = true;
            self.player.shrink_timer = 0.25;
        }

        if self.player.shrunk {
            self.player.shrink_timer -= dt;
            if self.player.shrink_timer <= 0.0 {
                self.player.shrunk = false;
                self.player.shrink_timer = 0.0;
            }
        }

        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            self.spawn_enemy();
            self.spawn_timer = SPAWN_INTERVAL;
        }

        for enemy in self.enemies.iter_mut() {
            if enemy.collides_with(&self.player) {
                enemy.die();
                self.health = self.health.saturating_sub(1);
                if self.health == 0 && !self.game_over {
                    self.game_over = true;
                    break;
                }
            }
            if enemy.dead {
                continue;
            }

            enemy.pos += enemy.vel * dt;

            //enemy.anim.tick(dt);
        }
        // gather_contacts(&mut triggers, &[prect], &door_rects);
        // if triggers.is_empty() {
        //     self.player.touching_door = false;
        // }
        // for (_player, _prect, door, _doorrect, _overlap) in triggers.drain(..) {
        //     // enter door if player has moved, wasn't previously touching door
        //     if !self.player.touching_door {
        //         self.player.touching_door = true;
        //         let (door_to, door_to_pos, _door_pos) = &self.doors[door];
        //         let dest = self
        //             .levels
        //             .iter()
        //             .position(|l| l.name() == door_to)
        //             .expect("door to invalid room {door_to}!");
        //         if dest == self.current_level {
        //             self.player.pos = self
        //                 .level()
        //                 .grid_to_world((door_to_pos.0 as usize, door_to_pos.1 as usize))
        //                 + Vec2 {
        //                     x: TILE_SZ as f32 / 2.0,
        //                     y: -12.0 + TILE_SZ as f32 / 2.0,
        //                 };
        //         } else {
        //             self.current_level = dest;
        //             self.enter_level(
        //                 self.level()
        //                     .grid_to_world((door_to_pos.0 as usize, door_to_pos.1 as usize))
        //                     + Vec2 {
        //                         x: 0.0 + TILE_SZ as f32 / 2.0,
        //                         y: -12.0 + TILE_SZ as f32 / 2.0,
        //                     },
        //             );
        //         }
        //         break;
        //     }
        // }
        // while self.player.pos.x
        //     > self.camera.screen_pos[0] + self.camera.screen_size[0] - SCREEN_FAST_MARGIN
        // {
        //     self.camera.screen_pos[0] += 1.0;
        // }
        // while self.player.pos.x < self.camera.screen_pos[0] + SCREEN_FAST_MARGIN {
        //     self.camera.screen_pos[0] -= 1.0;
        // }
        // while self.player.pos.y
        //     > self.camera.screen_pos[1] + self.camera.screen_size[1] - SCREEN_FAST_MARGIN
        // {
        //     self.camera.screen_pos[1] += 1.0;
        // }
        // while self.player.pos.y < self.camera.screen_pos[1] + SCREEN_FAST_MARGIN {
        //     self.camera.screen_pos[1] -= 1.0;
        // }
        // self.camera.screen_pos[0] =
        //     self.camera.screen_pos[0].clamp(0.0, (lw * TILE_SZ).max(W) as f32 - W as f32);
        // self.camera.screen_pos[1] =
        //     self.camera.screen_pos[1].clamp(0.0, (lh * TILE_SZ).max(H) as f32 - H as f32);
    }

    fn save_score(initials: &str, duration: u64) -> io::Result<()> {
        let path = Path::new("leaderboard.txt");
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(path)?;

        writeln!(file, "{},{}", initials, duration)
    }

    fn read_leaderboard() -> io::Result<Vec<(String, u64)>> {
        let path = Path::new("leaderboard.txt");
        let file = fs::File::open(path)?;
        let buf_reader = BufReader::new(file);
        let mut leaderboard = vec![];

        for line in buf_reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() == 2 {
                if let Ok(duration) = parts[1].parse::<u64>() {
                    leaderboard.push((parts[0].to_string(), duration));
                }
            }
        }

        leaderboard.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(leaderboard)
    }

    fn display_leaderboard(leaderboard: &[(String, u64)]) {
        println!("Leaderboard");
        println!("----------------");
        println!("Initials\t\tDuration (Seconds)");
        for (initials, duration) in leaderboard {
            println!("{}\t\t\t{}", initials, duration);
        }
    }

    fn prompt_for_initials() -> String {
        println!("Enter your initials:");
        let mut initials = String::new();
        io::stdin()
            .read_line(&mut initials)
            .expect("Failed to read line");
        initials.trim().to_uppercase()
    }

    fn end_game(&mut self) {
        println!("Game Over!");
        let duration = self.game_start_time.elapsed().as_secs();
        println!("You survived for {} seconds.", duration);

        let initials = Self::prompt_for_initials();
        if let Err(e) = Self::save_score(&initials, duration) {
            eprintln!("Error saving score: {}", e);
        }

        match Self::read_leaderboard() {
            Ok(leaderboard) => {
                Self::display_leaderboard(&leaderboard);
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error reading leaderboard: {}", e);
                std::process::exit(1);
            }
        }
    }
}
