use crate::game::Game;
use crate::timeline::TimelineStats;
use crate::world::{
    BONUS_PUZZLES, BONUS_TERMINALS, FixedPointState, GameState, PLAYER_WIDTH, STAGES, Stage,
    WORLD_HEIGHT, WORLD_WIDTH, is_bonus_stage,
};
use macroquad::prelude::*;

pub fn responsive_world_camera() -> Camera2D {
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

pub fn render_world(game: &Game, muted: bool) {
    let current_stage = game.current_stage();
    draw_background(current_stage, game.rewind_active);
    draw_level_geometry(current_stage);
    if is_bonus_stage(game.stage_index) {
        draw_bonus_terminals(game);
    }
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
    if game.bonus_notify_timer > 0.0 {
        draw_bonus_notification(game);
    }
    if game.editor_is_open() {
        draw_editor(game);
    } else if is_bonus_stage(game.stage_index) {
        if let Some(terminal) = game.nearby_bonus_terminal() {
            let rect = BONUS_TERMINALS[terminal];
            centered_text(
                if game.bonus_solved[terminal] {
                    "SOLVED"
                } else {
                    "E / HACK"
                },
                rect.x + rect.w * 0.5,
                rect.y - 14.0,
                16,
                if game.bonus_solved[terminal] {
                    color(110, 220, 140, 255)
                } else {
                    color(255, 220, 110, 255)
                },
            );
        }
    }

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

fn draw_bonus_terminals(game: &Game) {
    for (index, &terminal) in BONUS_TERMINALS.iter().enumerate() {
        let solved = game.bonus_solved[index];
        let pulse = 0.5 + 0.5 * (get_time() as f32 * 2.6 + index as f32 * 1.3).sin();
        let base = if solved {
            color(80, 190, 120, 255)
        } else {
            color(70, 170, 190, 255)
        };
        let glow = if solved {
            color(80, 190, 120, (30.0 + pulse * 22.0) as u8)
        } else {
            color(70, 170, 190, (18.0 + pulse * 18.0) as u8)
        };
        let center = vec2(terminal.x + terminal.w * 0.5, terminal.y + 22.0);
        draw_circle(center.x, center.y, 30.0 + pulse * 4.0, glow);
        draw_rectangle(
            terminal.x + 6.0,
            terminal.y + terminal.h - 10.0,
            terminal.w - 12.0,
            10.0,
            color(32, 38, 48, 255),
        );
        draw_rectangle(
            terminal.x,
            terminal.y,
            terminal.w,
            terminal.h,
            color(22, 28, 38, 220),
        );
        draw_rectangle_lines(terminal.x, terminal.y, terminal.w, terminal.h, 2.0, base);
        // Screen
        draw_rectangle(
            terminal.x + 6.0,
            terminal.y + 6.0,
            terminal.w - 12.0,
            28.0,
            if solved {
                color(28, 52, 42, 255)
            } else {
                color(28, 42, 52, 255)
            },
        );
        let icon = if solved { "✓" } else { ">" };
        centered_text(icon, center.x, terminal.y + 24.0, 22, base);
        // Label
        let label = BONUS_PUZZLES[index].title;
        small_text(label, terminal.x - 6.0, terminal.y - 10.0, base);
    }
}

fn draw_bonus_notification(game: &Game) {
    let alpha = (game.bonus_notify_timer * 1.2).min(1.0);
    let bg = color(10, 16, 28, (210.0 * alpha) as u8);
    let fg = color(180, 230, 255, (255.0 * alpha) as u8);
    draw_rectangle(WORLD_WIDTH * 0.5 - 320.0, 142.0, 640.0, 32.0, bg);
    draw_rectangle_lines(
        WORLD_WIDTH * 0.5 - 320.0,
        142.0,
        640.0,
        32.0,
        1.0,
        color(90, 180, 220, (180.0 * alpha) as u8),
    );
    centered_text(&game.bonus_notify_text, WORLD_WIDTH * 0.5, 162.0, 16, fg);
}

fn draw_editor(game: &Game) {
    draw_rectangle(0.0, 0.0, WORLD_WIDTH, WORLD_HEIGHT, color(6, 9, 18, 210));
    let puzzle = &BONUS_PUZZLES[game.editor_terminal()];
    let panel_x = 128.0;
    let panel_y = 72.0;
    let panel_w = WORLD_WIDTH - panel_x * 2.0;
    let panel_h = WORLD_HEIGHT - panel_y * 2.0;
    draw_rectangle(panel_x, panel_y, panel_w, panel_h, color(12, 16, 28, 245));
    draw_rectangle_lines(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        2.0,
        color(90, 160, 200, 220),
    );
    draw_rectangle(panel_x, panel_y, panel_w, 34.0, color(18, 28, 44, 255));
    small_text(
        &format!(
            "BONUS TERMINAL {}/3 - {}  // {}",
            game.editor_terminal() + 1,
            puzzle.title,
            puzzle.subtitle
        ),
        panel_x + 16.0,
        panel_y + 22.0,
        color(160, 210, 240, 255),
    );
    small_text(
        "ESC / CLOSE   Ctrl+Enter OR F5 / CHECK",
        panel_x + panel_w - 268.0,
        panel_y + 22.0,
        color(90, 130, 150, 255),
    );
    // Prompt
    let prompt_y = panel_y + 56.0;
    let mut y = prompt_y;
    for line in puzzle.prompt.split('\n') {
        small_text(line, panel_x + 16.0, y, color(180, 200, 215, 255));
        y += 16.0;
    }
    y += 6.0;
    draw_line(
        panel_x + 16.0,
        y,
        panel_x + panel_w - 16.0,
        y,
        1.0,
        color(50, 70, 90, 120),
    );
    y += 14.0;
    // Code area
    draw_rectangle(
        panel_x + 12.0,
        y,
        panel_w - 24.0,
        320.0,
        color(8, 12, 20, 255),
    );
    draw_rectangle_lines(
        panel_x + 12.0,
        y,
        panel_w - 24.0,
        320.0,
        1.0,
        color(60, 80, 100, 160),
    );
    let mut code_y = y + 18.0;
    let lines: Vec<&str> = game.editor_text().split('\n').collect();
    for line in &lines {
        // Simple tab expansion for display
        let display = line.replace('\t', "    ");
        small_text(&display, panel_x + 22.0, code_y, color(210, 225, 235, 255));
        code_y += 15.0;
        if code_y > y + 308.0 {
            break;
        }
    }
    // Cursor
    let cursor_blink = ((get_time() * 2.0) as i32) % 2 == 0;
    if cursor_blink {
        let last_line = lines.last().unwrap_or(&"");
        let cursor_x = panel_x + 22.0 + last_line.len() as f32 * 7.2;
        let cursor_y = code_y - 15.0;
        draw_rectangle(
            cursor_x,
            cursor_y - 10.0,
            8.0,
            12.0,
            color(160, 220, 255, 180),
        );
    }
    let status_y = y + 338.0;
    let status_color = if game.editor_status_is_error() {
        color(255, 120, 120, 255)
    } else if game.bonus_solved[game.editor_terminal()] {
        color(120, 230, 140, 255)
    } else {
        color(150, 180, 200, 255)
    };
    small_text(game.editor_status(), panel_x + 16.0, status_y, status_color);
    small_text(
        puzzle.hint,
        panel_x + 16.0,
        status_y + 16.0,
        color(90, 120, 140, 255),
    );
    if game.bonus_solved[game.editor_terminal()] {
        centered_text(
            "SOLVED - Press Esc to return to the timeline.",
            WORLD_WIDTH * 0.5,
            panel_y + panel_h - 18.0,
            15,
            color(120, 230, 140, 255),
        );
    }
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

    let (step, objective, objective_color) = if is_bonus_stage(game.stage_index) {
        let solved = game.bonus_solved.iter().filter(|&&v| v).count();
        if solved < 3 {
            (
                "01",
                format!("HACK {}/3 TERMINALS  -  E TO EDIT RUST", solved),
                color(90, 200, 210, 255),
            )
        } else if !game.fixed.door_latched {
            (
                "02",
                "HOLD R / REVERSE THE GATE".to_owned(),
                color(225, 91, 177, 255),
            )
        } else {
            (
                "03",
                "THE FORGE IS OPEN / REACH EXIT".to_owned(),
                color(83, 224, 220, 255),
            )
        }
    } else if !game.fixed.door_armed {
        (
            "01",
            "TOUCH THE FIXED POINT".to_owned(),
            color(244, 195, 83, 255),
        )
    } else if !game.fixed.door_latched {
        (
            "02",
            "HOLD R / REVERSE THE GATE".to_owned(),
            color(225, 91, 177, 255),
        )
    } else {
        (
            "03",
            "THE FUTURE IS OPEN / REACH EXIT".to_owned(),
            color(83, 224, 220, 255),
        )
    };
    let width = 535.0;
    let x = WORLD_WIDTH - width - 28.0;
    draw_rectangle(x, 22.0, width, 58.0, panel);
    draw_rectangle(x, 22.0, 6.0, 58.0, objective_color);
    small_text(step, x + 21.0, 59.0, color(146, 172, 188, 255));
    world_text(&objective, x + 61.0, 61.0, 25, objective_color);

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
