use std::collections::VecDeque;

use macroquad::prelude::*;
use unrun::timeline::{Timeline, TimelineStats};
use unrun::world::{
    BEACON, DOOR_X, FIRST_BLOCK, FIXED_DT, FLOOR, FixedPointState, GOAL, GameState, HISTORY_FRAMES,
    InputFrame, LAST_BLOCK, PLAYER_WIDTH, WORLD_HEIGHT, WORLD_WIDTH, static_solids,
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

impl Game {
    fn new() -> Self {
        let state = GameState::default();
        let timeline = Timeline::new(&state, HISTORY_FRAMES, CHECKPOINT_INTERVAL)
            .expect("the initial game state must be snapshot-compatible");
        Self {
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

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn update(&mut self, frame_dt: f32) {
        if is_key_pressed(KeyCode::Backspace)
            || (self.state.completed && is_key_pressed(KeyCode::Enter))
        {
            self.reset();
            return;
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
            return;
        }

        let horizontal = axis(
            is_key_down(KeyCode::A) || is_key_down(KeyCode::Left),
            is_key_down(KeyCode::D) || is_key_down(KeyCode::Right),
        );
        self.accumulator += frame_dt;
        self.rewind_active = false;

        let mut steps = 0;
        while self.accumulator >= FIXED_DT && steps < MAX_FRAME_STEPS {
            if rewind_requested {
                let old_player = self.state.player;
                match self.timeline.rewind(&mut self.state) {
                    Ok(true) => {
                        self.fixed.step_rewind(FIXED_DT);
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
                self.state.step(input, &mut self.fixed, FIXED_DT);
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
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut game = Game::new();
    // Enables deterministic visual smoke tests without changing normal play.
    let capture_path = std::env::var("UNRUN_CAPTURE_PATH").ok();
    let mut rendered_frames = 0;
    loop {
        game.update(get_frame_time());

        clear_background(color(3, 4, 12, 255));
        let world_camera = responsive_world_camera();
        set_camera(&world_camera);
        render_world(&game);
        set_default_camera();
        rendered_frames += 1;
        if rendered_frames == 3 {
            if let Some(path) = &capture_path {
                export_screen_png(path);
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
    Camera2D::from_display_rect(display)
}

fn export_screen_png(path: &str) {
    let mut image = get_screen_data();
    let row_bytes = image.width() * 4;
    for y in 0..image.height() / 2 {
        let top = y * row_bytes;
        let bottom = (image.height() - y - 1) * row_bytes;
        let (upper, lower) = image.bytes.split_at_mut(bottom);
        upper[top..top + row_bytes].swap_with_slice(&mut lower[..row_bytes]);
    }
    image.export_png(path);
}

fn render_world(game: &Game) {
    draw_background(game.rewind_active);
    draw_level_geometry();
    draw_time_cable(&game.fixed);
    draw_beacon(&game.fixed);
    draw_gate(&game.fixed);
    draw_goal(game.state.completed);

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
    draw_world_labels(&game.fixed);

    if game.state.completed {
        draw_completion(game);
    }
}

fn draw_background(rewinding: bool) {
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
        draw_line(x as f32, 0.0, x as f32, FLOOR.y, 1.0, color(37, 55, 91, 35));
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
    for ring in 0..6 {
        draw_circle_lines(
            DOOR_X + 21.0,
            315.0,
            85.0 + ring as f32 * 28.0,
            1.0,
            Color::new(0.23, 0.15, 0.42, 0.16 + halo * 0.025),
        );
    }
    draw_line(
        DOOR_X + 21.0,
        72.0,
        DOOR_X + 21.0,
        520.0,
        1.0,
        color(131, 89, 204, 35),
    );
}

fn draw_level_geometry() {
    for solid in static_solids() {
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

    draw_rectangle(0.0, FLOOR.y + 62.0, WORLD_WIDTH, 38.0, color(6, 8, 17, 255));
    for x in (18..1280).step_by(72) {
        draw_rectangle(x as f32, FLOOR.y + 19.0, 2.0, 17.0, color(65, 83, 106, 100));
    }

    draw_rectangle_lines(
        FIRST_BLOCK.x + 12.0,
        FIRST_BLOCK.y + 16.0,
        FIRST_BLOCK.w - 24.0,
        28.0,
        2.0,
        color(71, 106, 137, 120),
    );
    draw_rectangle_lines(
        LAST_BLOCK.x + 12.0,
        LAST_BLOCK.y + 14.0,
        LAST_BLOCK.w - 24.0,
        22.0,
        2.0,
        color(71, 106, 137, 120),
    );
}

fn draw_time_cable(fixed: &FixedPointState) {
    let cable_color = if fixed.door_armed {
        color(248, 194, 74, 210)
    } else {
        color(90, 78, 79, 110)
    };
    let y = FLOOR.y - 8.0;
    draw_line(BEACON.x + BEACON.w, y, DOOR_X, y, 3.0, cable_color);
    for x in (570..790).step_by(28) {
        let radius = if fixed.door_armed { 3.0 } else { 2.0 };
        draw_circle(x as f32, y, radius, cable_color);
    }
}

fn draw_beacon(fixed: &FixedPointState) {
    let pulse = 0.5 + 0.5 * (get_time() as f32 * 3.2).sin();
    let center = vec2(BEACON.x + BEACON.w * 0.5, BEACON.y + 22.0);
    let glow = if fixed.door_armed {
        color(255, 203, 80, (45.0 + pulse * 40.0) as u8)
    } else {
        color(129, 103, 91, 35)
    };
    draw_circle(center.x, center.y, 32.0 + pulse * 5.0, glow);
    draw_rectangle(
        BEACON.x + 7.0,
        FLOOR.y - 12.0,
        BEACON.w - 14.0,
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

fn draw_gate(fixed: &FixedPointState) {
    let rect = fixed.door_rect();
    let active = fixed.door_armed;
    let gate_color = if fixed.door_latched {
        color(245, 195, 72, 255)
    } else if active {
        color(205, 75, 159, 255)
    } else {
        color(107, 53, 99, 255)
    };

    draw_rectangle(DOOR_X - 10.0, 82.0, 62.0, 8.0, color(38, 29, 52, 255));
    draw_line(DOOR_X, 82.0, DOOR_X, FLOOR.y, 2.0, color(119, 65, 135, 100));
    draw_line(
        DOOR_X + 42.0,
        82.0,
        DOOR_X + 42.0,
        FLOOR.y,
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

    draw_rectangle(DOOR_X - 18.0, 188.0, 5.0, 244.0, color(25, 27, 43, 220));
    draw_rectangle(
        DOOR_X - 18.0,
        432.0 - 244.0 * fixed.door_open,
        5.0,
        244.0 * fixed.door_open,
        gate_color,
    );
    for tick in 0..=4 {
        let y = 188.0 + tick as f32 * 61.0;
        draw_line(
            DOOR_X - 23.0,
            y,
            DOOR_X - 10.0,
            y,
            1.0,
            color(119, 89, 131, 160),
        );
    }
}

fn draw_goal(completed: bool) {
    let center = vec2(GOAL.x + GOAL.w * 0.5, GOAL.y + GOAL.h * 0.52);
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
            GOAL.x + amount,
            GOAL.y + amount,
            GOAL.w - amount * 2.0,
            GOAL.h - amount,
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
    let panel = color(7, 10, 23, 210);
    draw_rectangle(28.0, 22.0, 470.0, 105.0, panel);
    draw_rectangle_lines(28.0, 22.0, 470.0, 105.0, 1.0, color(75, 107, 139, 120));

    small_text("TIMELINE / 20 SEC", 45.0, 50.0, color(171, 200, 214, 255));
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

fn draw_world_labels(fixed: &FixedPointState) {
    small_text("A / D", 82.0, 592.0, color(119, 149, 164, 220));
    small_text("MOVE", 78.0, 608.0, color(63, 89, 105, 220));
    small_text(
        "SPACE",
        FIRST_BLOCK.x + 13.0,
        FIRST_BLOCK.y - 24.0,
        color(119, 149, 164, 220),
    );
    small_text(
        "JUMP",
        FIRST_BLOCK.x + 18.0,
        FIRST_BLOCK.y - 8.0,
        color(63, 89, 105, 220),
    );
    small_text(
        if fixed.door_armed {
            "FIXED / REMEMBERED"
        } else {
            "FIXED POINT"
        },
        BEACON.x - 28.0,
        BEACON.y - 18.0,
        if fixed.door_armed {
            color(251, 205, 100, 245)
        } else {
            color(121, 109, 107, 210)
        },
    );
    if fixed.door_armed && !fixed.door_latched {
        centered_text(
            "HOLD  R",
            DOOR_X + 21.0,
            470.0,
            25,
            color(226, 91, 178, 240),
        );
        centered_text(
            "THIS GATE RUNS BACKWARD",
            DOOR_X + 21.0,
            495.0,
            15,
            color(141, 89, 139, 230),
        );
    }
    small_text(
        "EXIT",
        GOAL.x + 18.0,
        GOAL.y - 12.0,
        color(97, 211, 214, 230),
    );
    small_text("BACKSPACE / RESET", 1090.0, 697.0, color(61, 79, 97, 180));
}

fn draw_completion(game: &Game) {
    draw_rectangle(0.0, 0.0, WORLD_WIDTH, WORLD_HEIGHT, color(5, 7, 17, 175));
    let x = 342.0;
    let y = 202.0;
    draw_rectangle(x, y, 596.0, 276.0, color(9, 13, 28, 245));
    draw_rectangle_lines(x, y, 596.0, 276.0, 2.0, color(247, 199, 88, 220));
    draw_rectangle(x, y, 596.0, 7.0, color(247, 199, 88, 255));
    centered_text(
        "PARADOX RESOLVED",
        WORLD_WIDTH * 0.5,
        y + 75.0,
        38,
        color(246, 211, 125, 255),
    );
    centered_text(
        "THE GATE REMEMBERED A FUTURE YOU ERASED.",
        WORLD_WIDTH * 0.5,
        y + 119.0,
        20,
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
        y + 164.0,
        18,
        color(92, 220, 222, 255),
    );
    centered_text(
        "ENTER / PLAY AGAIN     R / REWIND THE ENDING",
        WORLD_WIDTH * 0.5,
        y + 226.0,
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
