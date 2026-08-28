use std::collections::VecDeque;

use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, set_sound_volume, stop_sound,
};
use macroquad::prelude::*;
use unrun::sound::{
    fixed_point_wav, gate_latched_wav, jump_wav, rewind_loop_wav, stage_clear_wav,
    uk_garage_loop_wav,
};
use unrun::timeline::{Timeline, TimelineStats};
use unrun::visual_test::{draw_orientation_probe, validate_orientation};
use unrun::world::{
    FIXED_DT, FixedPointState, GameState, HISTORY_FRAMES, InputFrame, PLAYER_WIDTH, STAGES, Stage,
    StepEvents, WORLD_HEIGHT, WORLD_WIDTH, stage,
};

const CHECKPOINT_INTERVAL: usize = 120;
const MAX_FRAME_STEPS: usize = 8;
const WINDOW_WIDTH: i32 = 1600;
const WINDOW_HEIGHT: i32 = 900;

fn window_conf() -> Conf {
    Conf {
        window_title: "UNRUN // FIXED POINT".to_owned(),
        window_width: WINDOW_WIDTH,
        window_height: WINDOW_HEIGHT,
        high_dpi: true,
        window_resizable: true,
        sample_count: 4,
        ..Default::default()
    }
}

#[derive(Clone, Copy)]
struct Ghost {
    position: Vec2,
    facing: f32,
    life: f32,
}

struct Game {
    stage_index: usize,
    state: GameState,
    fixed: FixedPointState,
    timeline: Timeline,
    accumulator: f32,
    jump_queued: bool,
    rewind_active: bool,
    rewind_blocked_flash: f32,
    ghost_timer: usize,
    ghosts: VecDeque<Ghost>,
}

#[derive(Default)]
struct GameEvents {
    jumped: bool,
    fixed_point_activated: bool,
    gate_latched: bool,
    completed: bool,
    stage_changed: bool,
}

impl GameEvents {
    fn include_step(&mut self, events: StepEvents) {
        self.jumped |= events.jumped;
        self.fixed_point_activated |= events.fixed_point_activated;
        self.completed |= events.completed;
    }
}

impl Game {
    fn new(stage_index: usize) -> Self {
        let stage_index = stage_index % STAGES.len();
        let state = GameState::new(stage(stage_index));
        let timeline = Timeline::new(&state, HISTORY_FRAMES, CHECKPOINT_INTERVAL)
            .expect("the initial game state must be snapshot-compatible");
        Self {
            stage_index,
            state,
            fixed: FixedPointState::default(),
            timeline,
            accumulator: 0.0,
            jump_queued: false,
            rewind_active: false,
            rewind_blocked_flash: 0.0,
            ghost_timer: 0,
            ghosts: VecDeque::new(),
        }
    }

    fn current_stage(&self) -> &'static Stage {
        stage(self.stage_index)
    }

    fn is_last_stage(&self) -> bool {
        self.stage_index + 1 == STAGES.len()
    }

    fn reset(&mut self) {
        *self = Self::new(self.stage_index);
    }

    fn update(&mut self, frame_dt: f32) -> GameEvents {
        let mut events = GameEvents::default();
        if is_key_pressed(KeyCode::Backspace) {
            self.reset();
            events.stage_changed = true;
            return events;
        }
        if self.state.completed && is_key_pressed(KeyCode::Enter) {
            *self = Self::new((self.stage_index + 1) % STAGES.len());
            events.stage_changed = true;
            return events;
        }

        let frame_dt = frame_dt.min(0.1);
        self.rewind_blocked_flash = (self.rewind_blocked_flash - frame_dt * 2.5).max(0.0);
        for ghost in &mut self.ghosts {
            ghost.life -= frame_dt * 1.9;
        }
        while self.ghosts.front().is_some_and(|ghost| ghost.life <= 0.0) {
            self.ghosts.pop_front();
        }

        let rewind_requested = is_key_down(KeyCode::R) || is_key_down(KeyCode::LeftShift);
        self.jump_queued |= is_key_pressed(KeyCode::Space)
            || is_key_pressed(KeyCode::W)
            || is_key_pressed(KeyCode::Up);

        if self.state.completed && !rewind_requested {
            self.accumulator = 0.0;
            self.rewind_active = false;
            return events;
        }

        let horizontal = axis(
            is_key_down(KeyCode::A) || is_key_down(KeyCode::Left),
            is_key_down(KeyCode::D) || is_key_down(KeyCode::Right),
        );
        self.accumulator += frame_dt;
        self.rewind_active = false;

        let mut steps = 0;
        let current_stage = stage(self.stage_index);
        while self.accumulator >= FIXED_DT && steps < MAX_FRAME_STEPS {
            if rewind_requested {
                let old_player = self.state.player;
                match self.timeline.rewind(&mut self.state) {
                    Ok(true) => {
                        events.gate_latched |= self.fixed.step_rewind(FIXED_DT);
                        self.rewind_active = true;
                        self.jump_queued = false;
                        self.ghost_timer += 1;
                        if self.ghost_timer % 3 == 0 {
                            self.ghosts.push_back(Ghost {
                                position: old_player.position,
                                facing: old_player.facing,
                                life: 1.0,
                            });
                            if self.ghosts.len() > 24 {
                                self.ghosts.pop_front();
                            }
                        }
                    }
                    Ok(false) => {
                        self.rewind_blocked_flash = 1.0;
                    }
                    Err(error) => panic!("timeline integrity failure: {error}"),
                }
            } else {
                self.fixed.step_forward(FIXED_DT);
                let input = InputFrame {
                    horizontal,
                    jump_pressed: self.jump_queued,
                };
                self.jump_queued = false;
                let step_events = self
                    .state
                    .step(input, &mut self.fixed, current_stage, FIXED_DT);
                events.include_step(step_events);
                self.timeline
                    .record(&self.state)
                    .expect("the game must always emit valid snapshots");
            }
            self.accumulator -= FIXED_DT;
            steps += 1;
        }
        if steps == MAX_FRAME_STEPS {
            self.accumulator = 0.0;
        }
        events
    }
}

struct AudioSystem {
    music: Sound,
    jump: Sound,
    fixed_point: Sound,
    gate_latched: Sound,
    stage_clear: Sound,
    rewind: Sound,
    muted: bool,
    rewind_playing: bool,
}

impl AudioSystem {
    async fn load() -> Result<Self, macroquad::Error> {
        let music = load_sound_from_bytes(&uk_garage_loop_wav()).await?;
        let jump = load_sound_from_bytes(&jump_wav()).await?;
        let fixed_point = load_sound_from_bytes(&fixed_point_wav()).await?;
        let gate_latched = load_sound_from_bytes(&gate_latched_wav()).await?;
        let stage_clear = load_sound_from_bytes(&stage_clear_wav()).await?;
        let rewind = load_sound_from_bytes(&rewind_loop_wav()).await?;
        play_sound(
            &music,
            PlaySoundParams {
                looped: true,
                volume: 0.42,
            },
        );
        Ok(Self {
            music,
            jump,
            fixed_point,
            gate_latched,
            stage_clear,
            rewind,
            muted: false,
            rewind_playing: false,
        })
    }

    fn update(&mut self, events: &GameEvents, rewinding: bool) {
        if is_key_pressed(KeyCode::M) {
            self.muted = !self.muted;
        }
        if events.jumped {
            self.play_once(&self.jump, 0.34);
        }
        if events.fixed_point_activated {
            self.play_once(&self.fixed_point, 0.58);
        }
        if events.gate_latched {
            self.play_once(&self.gate_latched, 0.62);
        }
        if events.completed {
            self.play_once(&self.stage_clear, 0.66);
        }

        if rewinding != self.rewind_playing {
            if rewinding {
                play_sound(
                    &self.rewind,
                    PlaySoundParams {
                        looped: true,
                        volume: if self.muted { 0.0 } else { 0.28 },
                    },
                );
            } else {
                stop_sound(&self.rewind);
            }
            self.rewind_playing = rewinding;
        }
        set_sound_volume(
            &self.music,
            if self.muted {
                0.0
            } else if rewinding {
                0.20
            } else {
                0.42
            },
        );
        if self.rewind_playing {
            set_sound_volume(&self.rewind, if self.muted { 0.0 } else { 0.28 });
        }
    }

    fn play_once(&self, sound: &Sound, volume: f32) {
        if !self.muted {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let visual_test = std::env::args().any(|argument| argument == "--visual-test");
    let mut game = Game::new(0);
    let mut audio = if visual_test {
        None
    } else {
        match AudioSystem::load().await {
            Ok(audio) => Some(audio),
            Err(error) => {
                eprintln!("audio disabled: {error}");
                None
            }
        }
    };
    // Enables deterministic visual smoke tests without changing normal play.
    let capture_path = std::env::var("UNRUN_CAPTURE_PATH").ok();
    let mut rendered_frames = 0;
    loop {
        clear_background(color(3, 4, 12, 255));
        let world_camera = responsive_world_camera();
        set_camera(&world_camera);
        if visual_test {
            draw_orientation_probe(WORLD_WIDTH, WORLD_HEIGHT);
        } else {
            let events = game.update(get_frame_time());
            if let Some(audio) = &mut audio {
                audio.update(&events, game.rewind_active);
            }
            render_world(&game, audio.as_ref().is_none_or(|audio| audio.muted));
        }
        set_default_camera();
        rendered_frames += 1;
        if rendered_frames == 3 {
            if visual_test {
                let image = get_screen_data();
                if let Some(path) = &capture_path {
                    image.export_png(path);
                }
                validate_orientation(&image)
                    .unwrap_or_else(|error| panic!("visual orientation test failed: {error}"));
                println!(
                    "visual orientation test passed at {}x{}",
                    image.width(),
                    image.height()
                );
                return;
            } else if let Some(path) = &capture_path {
                get_screen_data().export_png(path);
                return;
            }
        }

        next_frame().await;
    }
}

fn responsive_world_camera() -> Camera2D {
    let screen_aspect = screen_width().max(1.0) / screen_height().max(1.0);
    let world_aspect = WORLD_WIDTH / WORLD_HEIGHT;
    let display = if screen_aspect > world_aspect {
        let width = WORLD_HEIGHT * screen_aspect;
        Rect::new((WORLD_WIDTH - width) * 0.5, 0.0, width, WORLD_HEIGHT)
    } else {
        let height = WORLD_WIDTH / screen_aspect;
        Rect::new(0.0, (WORLD_HEIGHT - height) * 0.5, WORLD_WIDTH, height)
    };
    Camera2D::from_display_rect(Rect::new(
        display.x,
        display.y + display.h,
        display.w,
        -display.h,
    ))
}

fn render_world(game: &Game, muted: bool) {
    let current_stage = game.current_stage();
    draw_background(current_stage, game.rewind_active);
    draw_level_geometry(current_stage);
    draw_time_cable(&game.fixed, current_stage);
    draw_beacon(&game.fixed, current_stage);
    draw_gate(&game.fixed, current_stage);
    draw_goal(current_stage, game.state.completed);

    for ghost in &game.ghosts {
        draw_player_at(
            ghost.position,
            ghost.facing,
            game.state.player.animation_phase,
            color(75, 230, 255, (ghost.life * 90.0) as u8),
            true,
        );
    }
    draw_player(&game.state);

    if game.rewind_active {
        draw_rewind_overlay();
    }
    draw_hud(game);
    draw_world_labels(game, muted);

    if game.state.completed {
        draw_completion(game);
    }
}

fn draw_background(stage: &Stage, rewinding: bool) {
    for band in 0..9 {
        let t = band as f32 / 8.0;
        draw_rectangle(
            0.0,
            t * WORLD_HEIGHT,
            WORLD_WIDTH,
            WORLD_HEIGHT / 8.0 + 1.0,
            Color::new(0.025 + t * 0.018, 0.03 + t * 0.025, 0.09 + t * 0.04, 1.0),
        );
    }

    let direction = if rewinding { -1.0 } else { 1.0 };
    let time = get_time() as f32 * direction;
    for index in 0..54 {
        let seed = index as f32;
        let x = ((index * 197 + 43) % 1260) as f32 + (time * (2.0 + seed % 4.0)).sin() * 3.0;
        let y = ((index * 83 + 71) % 500) as f32 + 45.0;
        let pulse = 0.35 + 0.3 * (time * 0.7 + seed * 1.91).sin().abs();
        draw_circle(
            x,
            y,
            1.0 + seed % 3.0 * 0.35,
            Color::new(0.35, 0.62, 0.88, pulse),
        );
    }

    for x in (0..=1280).step_by(64) {
        draw_line(
            x as f32,
            0.0,
            x as f32,
            stage.base_floor_y,
            1.0,
            color(37, 55, 91, 35),
        );
    }
    for y in (44..=620).step_by(64) {
        draw_line(
            0.0,
            y as f32,
            WORLD_WIDTH,
            y as f32,
            1.0,
            color(37, 55, 91, 28),
        );
    }

    let halo = 0.5 + 0.5 * (get_time() as f32 * 0.7).sin();
    let halo_y = stage.door_floor_y - 305.0;
    for ring in 0..6 {
        draw_circle_lines(
            stage.door_x + 21.0,
            halo_y,
            85.0 + ring as f32 * 28.0,
            1.0,
            Color::new(0.23, 0.15, 0.42, 0.16 + halo * 0.025),
        );
    }
    draw_line(
        stage.door_x + 21.0,
        (stage.door_floor_y - 548.0).max(18.0),
        stage.door_x + 21.0,
        stage.door_floor_y - 100.0,
        1.0,
        color(131, 89, 204, 35),
    );
}

fn draw_level_geometry(stage: &Stage) {
    for &solid in stage.solids {
        draw_rectangle(solid.x, solid.y, solid.w, solid.h, color(17, 24, 43, 255));
        draw_rectangle(solid.x, solid.y, solid.w, 5.0, color(61, 88, 120, 255));
        draw_line(
            solid.x,
            solid.y + 8.0,
            solid.x + solid.w,
            solid.y + 8.0,
            1.0,
            color(112, 173, 193, 55),
        );
        let mut stripe = solid.x - solid.h;
        while stripe < solid.x + solid.w {
            draw_line(
                stripe,
                solid.y + solid.h,
                stripe + solid.h,
                solid.y,
                1.0,
                color(35, 49, 70, 100),
            );
            stripe += 28.0;
        }
    }

    draw_rectangle(
        0.0,
        stage.base_floor_y + 62.0,
        WORLD_WIDTH,
        38.0,
        color(6, 8, 17, 255),
    );
    for x in (18..1280).step_by(72) {
        draw_rectangle(
            x as f32,
            stage.base_floor_y + 19.0,
            2.0,
            17.0,
            color(65, 83, 106, 100),
        );
    }

    for &solid in stage.solids.iter().skip(1) {
        if solid.w > 36.0 && solid.h > 28.0 {
            draw_rectangle_lines(
                solid.x + 12.0,
                solid.y + 12.0,
                solid.w - 24.0,
                (solid.h - 24.0).max(8.0),
                2.0,
                color(71, 106, 137, 120),
            );
        }
    }
}

fn draw_time_cable(fixed: &FixedPointState, stage: &Stage) {
    let cable_color = if fixed.door_armed {
        color(248, 194, 74, 210)
    } else {
        color(90, 78, 79, 110)
    };
    let start = vec2(
        stage.beacon.x + stage.beacon.w * 0.5,
        stage.beacon.y + stage.beacon.h - 8.0,
    );
    let end = vec2(stage.door_x + 21.0, stage.door_floor_y - 8.0);
    draw_line(start.x, start.y, end.x, start.y, 3.0, cable_color);
    draw_line(end.x, start.y, end.x, end.y, 3.0, cable_color);
}

fn draw_beacon(fixed: &FixedPointState, stage: &Stage) {
    let beacon = stage.beacon;
    let pulse = 0.5 + 0.5 * (get_time() as f32 * 3.2).sin();
    let center = vec2(beacon.x + beacon.w * 0.5, beacon.y + 22.0);
    let glow = if fixed.door_armed {
        color(255, 203, 80, (45.0 + pulse * 40.0) as u8)
    } else {
        color(129, 103, 91, 35)
    };
    draw_circle(center.x, center.y, 32.0 + pulse * 5.0, glow);
    draw_rectangle(
        beacon.x + 7.0,
        beacon.y + beacon.h - 12.0,
        beacon.w - 14.0,
        12.0,
        color(52, 48, 48, 255),
    );
    draw_poly(
        center.x,
        center.y,
        4,
        18.0,
        45.0,
        if fixed.door_armed {
            color(255, 205, 88, 255)
        } else {
            color(116, 102, 101, 255)
        },
    );
    draw_poly_lines(
        center.x,
        center.y,
        4,
        24.0 + pulse * 2.0,
        45.0,
        2.0,
        color(255, 221, 133, if fixed.door_armed { 180 } else { 55 }),
    );
    draw_circle(center.x, center.y, 4.0, color(255, 245, 205, 255));
}

fn draw_gate(fixed: &FixedPointState, stage: &Stage) {
    let door_x = stage.door_x;
    let rect = fixed.door_rect(stage);
    let active = fixed.door_armed;
    let gate_color = if fixed.door_latched {
        color(245, 195, 72, 255)
    } else if active {
        color(205, 75, 159, 255)
    } else {
        color(107, 53, 99, 255)
    };

    let rail_top = (stage.door_floor_y - 538.0).max(20.0);
    draw_rectangle(door_x - 10.0, rail_top, 62.0, 8.0, color(38, 29, 52, 255));
    draw_line(
        door_x,
        rail_top,
        door_x,
        stage.door_floor_y,
        2.0,
        color(119, 65, 135, 100),
    );
    draw_line(
        door_x + 42.0,
        rail_top,
        door_x + 42.0,
        stage.door_floor_y,
        2.0,
        color(119, 65, 135, 100),
    );
    draw_rectangle(
        rect.x - 7.0,
        rect.y - 5.0,
        rect.w + 14.0,
        rect.h + 10.0,
        Color::new(gate_color.r, gate_color.g, gate_color.b, 0.12),
    );
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, color(26, 16, 40, 255));
    draw_rectangle(rect.x + 4.0, rect.y, 5.0, rect.h, gate_color);
    draw_rectangle(rect.x + rect.w - 9.0, rect.y, 5.0, rect.h, gate_color);

    let first_segment = (rect.y / 34.0).floor() as i32 - 1;
    let last_segment = ((rect.y + rect.h) / 34.0).ceil() as i32 + 1;
    for segment in first_segment..=last_segment {
        let y = segment as f32 * 34.0;
        if y >= rect.y && y <= rect.y + rect.h {
            draw_line(rect.x + 8.0, y, rect.x + rect.w - 8.0, y, 2.0, gate_color);
            draw_circle(rect.x + rect.w * 0.5, y + 17.0, 2.0, gate_color);
        }
    }

    let gauge_top = (stage.door_floor_y - 432.0).max(90.0);
    let gauge_bottom = (stage.door_floor_y - 188.0).max(gauge_top + 100.0);
    let gauge_height = gauge_bottom - gauge_top;
    draw_rectangle(
        door_x - 18.0,
        gauge_top,
        5.0,
        gauge_height,
        color(25, 27, 43, 220),
    );
    draw_rectangle(
        door_x - 18.0,
        gauge_bottom - gauge_height * fixed.door_open,
        5.0,
        gauge_height * fixed.door_open,
        gate_color,
    );
    for tick in 0..=4 {
        let y = gauge_top + tick as f32 * gauge_height / 4.0;
        draw_line(
            door_x - 23.0,
            y,
            door_x - 10.0,
            y,
            1.0,
            color(119, 89, 131, 160),
        );
    }
}

fn draw_goal(stage: &Stage, completed: bool) {
    let goal = stage.goal;
    let center = vec2(goal.x + goal.w * 0.5, goal.y + goal.h * 0.52);
    let pulse = 0.5 + 0.5 * (get_time() as f32 * 2.1).sin();
    let glow = if completed {
        color(255, 210, 93, 85)
    } else {
        color(72, 215, 220, (35.0 + pulse * 25.0) as u8)
    };
    draw_circle(center.x, center.y, 54.0 + pulse * 4.0, glow);
    for inset in 0..3 {
        let amount = inset as f32 * 8.0;
        draw_rectangle_lines(
            goal.x + amount,
            goal.y + amount,
            goal.w - amount * 2.0,
            goal.h - amount,
            2.0,
            if completed {
                color(255, 213, 105, 220 - inset * 45)
            } else {
                color(86, 220, 224, 210 - inset * 45)
            },
        );
    }
    draw_circle(
        center.x,
        center.y,
        8.0 + pulse * 3.0,
        color(219, 252, 249, 220),
    );
}

fn draw_player(state: &GameState) {
    let player = state.player;
    draw_player_at(
        player.position,
        player.facing,
        player.animation_phase,
        color(222, 244, 243, 255),
        false,
    );
}

fn draw_player_at(position: Vec2, facing: f32, phase: f32, tint: Color, outline_only: bool) {
    let x = position.x;
    let y = position.y;
    let stride = phase.sin() * 5.0;
    let scarf_root = vec2(x + PLAYER_WIDTH * 0.5 - facing * 3.0, y + 15.0);
    draw_triangle(
        scarf_root,
        vec2(
            scarf_root.x - facing * 28.0,
            scarf_root.y - 5.0 + stride * 0.2,
        ),
        vec2(
            scarf_root.x - facing * 23.0,
            scarf_root.y + 7.0 + stride * 0.3,
        ),
        Color::new(0.93, 0.25, 0.46, tint.a),
    );

    if outline_only {
        draw_rectangle_lines(x + 5.0, y + 17.0, 20.0, 25.0, 2.0, tint);
        draw_circle_lines(x + 15.0, y + 10.0, 9.0, 2.0, tint);
        return;
    }

    draw_rectangle(x + 5.0, y + 17.0, 20.0, 25.0, color(25, 33, 49, 255));
    draw_rectangle_lines(x + 5.0, y + 17.0, 20.0, 25.0, 2.0, tint);
    draw_circle(x + 15.0, y + 10.0, 9.0, color(24, 31, 46, 255));
    draw_circle_lines(x + 15.0, y + 10.0, 9.0, 2.0, tint);
    draw_circle(x + 15.0 + facing * 3.2, y + 8.5, 1.7, tint);
    draw_line(x + 9.0, y + 41.0, x + 8.0 + stride, y + 48.0, 3.0, tint);
    draw_line(x + 21.0, y + 41.0, x + 22.0 - stride, y + 48.0, 3.0, tint);
}

fn draw_rewind_overlay() {
    draw_rectangle(0.0, 0.0, WORLD_WIDTH, WORLD_HEIGHT, color(22, 127, 166, 24));
    for y in (0..720).step_by(16) {
        draw_rectangle(0.0, y as f32, WORLD_WIDTH, 1.0, color(73, 216, 235, 22));
    }
    for x in (30..1280).step_by(110) {
        draw_triangle(
            vec2(x as f32, 94.0),
            vec2(x as f32 + 18.0, 84.0),
            vec2(x as f32 + 18.0, 104.0),
            color(95, 235, 244, 65),
        );
    }
    draw_rectangle_lines(
        8.0,
        8.0,
        WORLD_WIDTH - 16.0,
        WORLD_HEIGHT - 16.0,
        3.0,
        color(74, 223, 239, 120),
    );
}

fn draw_hud(game: &Game) {
    let stats = game.timeline.stats();
    let stage = game.current_stage();
    let panel = color(7, 10, 23, 210);
    draw_rectangle(28.0, 22.0, 470.0, 105.0, panel);
    draw_rectangle_lines(28.0, 22.0, 470.0, 105.0, 1.0, color(75, 107, 139, 120));

    small_text(
        &format!(
            "STAGE {}/{}  {}",
            game.stage_index + 1,
            STAGES.len(),
            stage.name
        ),
        45.0,
        50.0,
        color(171, 200, 214, 255),
    );
    let bar_x = 45.0;
    let bar_y = 66.0;
    let bar_w = 330.0;
    let fill = stats.rewindable_frames as f32 / stats.capacity_frames.max(1) as f32;
    draw_rectangle(bar_x, bar_y, bar_w, 12.0, color(28, 36, 57, 255));
    draw_rectangle(
        bar_x,
        bar_y,
        bar_w * fill,
        12.0,
        if game.rewind_active {
            color(76, 228, 240, 255)
        } else {
            color(103, 148, 184, 255)
        },
    );
    for tick in 0..=10 {
        let x = bar_x + tick as f32 * bar_w / 10.0;
        draw_line(x, bar_y, x, bar_y + 12.0, 1.0, color(8, 12, 25, 160));
    }
    small_text(
        &format!("{:.1}s", stats.rewindable_frames as f32 / 60.0),
        392.0,
        79.0,
        color(214, 234, 237, 255),
    );
    small_text(
        &format!(
            "CAS  {} BLOBS  /  {}  /  DELTA SAVE {:>2.0}%",
            stats.blob_count,
            format_bytes(stats.stored_payload_bytes),
            stats.payload_saving_percent()
        ),
        45.0,
        108.0,
        color(121, 158, 178, 255),
    );

    let (step, objective, objective_color) = if !game.fixed.door_armed {
        ("01", "TOUCH THE FIXED POINT", color(244, 195, 83, 255))
    } else if !game.fixed.door_latched {
        ("02", "HOLD R / REVERSE THE GATE", color(225, 91, 177, 255))
    } else {
        (
            "03",
            "THE FUTURE IS OPEN / REACH EXIT",
            color(83, 224, 220, 255),
        )
    };
    let width = 535.0;
    let x = WORLD_WIDTH - width - 28.0;
    draw_rectangle(x, 22.0, width, 58.0, panel);
    draw_rectangle(x, 22.0, 6.0, 58.0, objective_color);
    small_text(step, x + 21.0, 59.0, color(146, 172, 188, 255));
    world_text(objective, x + 61.0, 61.0, 25, objective_color);

    if game.rewind_active {
        centered_text(
            "< <  REWINDING  < <",
            WORLD_WIDTH * 0.5,
            126.0,
            23,
            color(99, 235, 244, 230),
        );
    } else if game.rewind_blocked_flash > 0.0 {
        centered_text(
            "BEGINNING OF TIMELINE",
            WORLD_WIDTH * 0.5,
            126.0,
            21,
            color(232, 102, 174, (game.rewind_blocked_flash * 255.0) as u8),
        );
    }
}

fn draw_world_labels(game: &Game, muted: bool) {
    let stage = game.current_stage();
    let fixed = &game.fixed;
    small_text(
        &format!("STAGE {}/{}", game.stage_index + 1, STAGES.len()),
        32.0,
        640.0,
        color(98, 125, 144, 180),
    );
    small_text(stage.subtitle, 32.0, 658.0, color(120, 150, 170, 190));
    small_text(
        "SPACE",
        stage.jump_hint[0],
        stage.jump_hint[1],
        color(119, 149, 164, 220),
    );
    small_text(
        "JUMP",
        stage.jump_hint[0] + 5.0,
        stage.jump_hint[1] + 16.0,
        color(63, 89, 105, 220),
    );
    small_text(
        if fixed.door_armed {
            "FIXED / REMEMBERED"
        } else {
            "FIXED POINT"
        },
        stage.beacon.x - 28.0,
        stage.beacon.y - 18.0,
        if fixed.door_armed {
            color(251, 205, 100, 245)
        } else {
            color(121, 109, 107, 210)
        },
    );
    if fixed.door_armed && !fixed.door_latched {
        let door_center = stage.door_x + 21.0;
        let hint_y = (stage.door_floor_y - 150.0).clamp(120.0, 520.0);
        centered_text("HOLD  R", door_center, hint_y, 25, color(226, 91, 178, 240));
        centered_text(
            "THIS GATE RUNS BACKWARD",
            door_center,
            hint_y + 25.0,
            15,
            color(141, 89, 139, 230),
        );
    }
    small_text(
        "EXIT",
        stage.goal.x + 18.0,
        stage.goal.y - 12.0,
        color(97, 211, 214, 230),
    );
    small_text("BACKSPACE / RESET", 1090.0, 682.0, color(61, 79, 97, 180));
    small_text(
        if muted { "M / UNMUTE" } else { "M / MUTE" },
        1090.0,
        697.0,
        color(61, 79, 97, 180),
    );
}

fn draw_completion(game: &Game) {
    draw_rectangle(0.0, 0.0, WORLD_WIDTH, WORLD_HEIGHT, color(5, 7, 17, 175));
    let x = 342.0;
    let y = 202.0;
    draw_rectangle(x, y, 596.0, 292.0, color(9, 13, 28, 245));
    draw_rectangle_lines(x, y, 596.0, 292.0, 2.0, color(247, 199, 88, 220));
    draw_rectangle(x, y, 596.0, 7.0, color(247, 199, 88, 255));
    let stage = game.current_stage();
    let title = if game.is_last_stage() {
        "PARADOX RESOLVED"
    } else {
        "STAGE CLEARED"
    };
    let subtitle = if game.is_last_stage() {
        "THE GATE REMEMBERED A FUTURE YOU ERASED."
    } else {
        "THE NEXT TIMELINE IS ALREADY OPEN."
    };
    let action = if game.is_last_stage() {
        "ENTER / REPLAY STAGE 01     R / REWIND THE ENDING"
    } else {
        "ENTER / NEXT STAGE     R / REWIND THIS ENDING"
    };
    centered_text(
        title,
        WORLD_WIDTH * 0.5,
        y + 75.0,
        38,
        color(246, 211, 125, 255),
    );
    centered_text(
        &format!(
            "{}  /  {}/{}",
            stage.name,
            game.stage_index + 1,
            STAGES.len()
        ),
        WORLD_WIDTH * 0.5,
        y + 105.0,
        15,
        color(148, 175, 190, 255),
    );
    centered_text(
        subtitle,
        WORLD_WIDTH * 0.5,
        y + 134.0,
        18,
        color(151, 181, 192, 255),
    );
    let stats: TimelineStats = game.timeline.stats();
    centered_text(
        &format!(
            "REWOUND {:.1}s    /    {} CONTENT-ADDRESSED BLOBS",
            game.fixed.rewound_frames as f32 / 60.0,
            stats.blob_count
        ),
        WORLD_WIDTH * 0.5,
        y + 176.0,
        18,
        color(92, 220, 222, 255),
    );
    centered_text(
        action,
        WORLD_WIDTH * 0.5,
        y + 238.0,
        18,
        color(213, 224, 219, 230),
    );
}

fn axis(negative: bool, positive: bool) -> f32 {
    f32::from(positive) - f32::from(negative)
}

fn color(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
    Color::from_rgba(red, green, blue, alpha)
}

fn small_text(text: &str, x: f32, y: f32, tint: Color) {
    world_text(text, x, y, 16, tint);
}

fn centered_text(text: &str, center_x: f32, baseline_y: f32, size: u16, tint: Color) {
    let (raster_size, font_scale, font_aspect) = camera_font_scale(size as f32);
    let dimensions = measure_text(text, None, raster_size.max(1), font_scale);
    world_text(
        text,
        center_x - dimensions.width * font_aspect * 0.5,
        baseline_y,
        size,
        tint,
    );
}

fn world_text(text: &str, x: f32, y: f32, size: u16, tint: Color) {
    let (raster_size, font_scale, font_scale_aspect) = camera_font_scale(size as f32);
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: raster_size.max(1),
            font_scale,
            font_scale_aspect,
            color: tint,
            ..Default::default()
        },
    );
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KiB", bytes as f32 / 1024.0)
    }
}
