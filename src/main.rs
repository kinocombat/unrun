use macroquad::prelude::*;
use unrun::audio::AudioSystem;
use unrun::editor::EditorInput;
use unrun::game::{FrameInput, Game};
use unrun::render::{render_world, responsive_world_camera};
use unrun::visual_test::{draw_orientation_probe, validate_orientation};
use unrun::world::{WORLD_HEIGHT, WORLD_WIDTH, is_bonus_stage};

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

#[macroquad::main(window_conf)]
async fn main() {
    let visual_test = std::env::args().any(|argument| argument == "--visual-test");
    let start_stage = std::env::var("UNRUN_START_STAGE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut game = match Game::new(start_stage) {
        Ok(game) => game,
        Err(error) => {
            show_fatal_error(&format!("INITIALIZATION FAILED: {error}")).await;
            return;
        }
    };
    if std::env::var("UNRUN_EDITOR_CAPTURE").is_ok() && is_bonus_stage(game.stage_index()) {
        game.open_editor(0);
    }
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
        clear_background(Color::from_rgba(3, 4, 12, 255));
        let world_camera = responsive_world_camera();
        set_camera(&world_camera);
        if visual_test {
            draw_orientation_probe(WORLD_WIDTH, WORLD_HEIGHT);
        } else {
            let input = collect_frame_input(game.editor_is_open());
            let events = match game.update(&input, get_frame_time()) {
                Ok(events) => events,
                Err(error) => {
                    show_fatal_error(&format!("TIMELINE FAILED: {error}")).await;
                    return;
                }
            };
            if let Some(audio) = &mut audio {
                audio.update(&events, game.is_rewinding(), input.mute_pressed);
            }
            render_world(&game, audio.as_ref().is_none_or(AudioSystem::is_muted));
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

fn collect_frame_input(editor_open: bool) -> FrameInput {
    let control_down = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
    let enter_pressed = is_key_pressed(KeyCode::Enter);
    let backspace_pressed = is_key_pressed(KeyCode::Backspace);
    let mut characters = String::new();
    if editor_open {
        while let Some(character) = get_char_pressed() {
            characters.push(character);
        }
    }

    FrameInput {
        horizontal: axis(
            is_key_down(KeyCode::A) || is_key_down(KeyCode::Left),
            is_key_down(KeyCode::D) || is_key_down(KeyCode::Right),
        ),
        jump_pressed: is_key_pressed(KeyCode::Space)
            || is_key_pressed(KeyCode::W)
            || is_key_pressed(KeyCode::Up),
        rewind_down: is_key_down(KeyCode::R) || is_key_down(KeyCode::LeftShift),
        reset_pressed: backspace_pressed,
        advance_pressed: enter_pressed,
        interact_pressed: is_key_pressed(KeyCode::E),
        mute_pressed: is_key_pressed(KeyCode::M),
        editor: EditorInput {
            submit: (control_down && enter_pressed) || is_key_pressed(KeyCode::F5),
            close: is_key_pressed(KeyCode::Escape),
            tab: is_key_pressed(KeyCode::Tab),
            enter: enter_pressed && !control_down,
            backspace: backspace_pressed,
            characters,
        },
    }
}

fn axis(negative: bool, positive: bool) -> f32 {
    f32::from(positive) - f32::from(negative)
}

async fn show_fatal_error(message: &str) {
    eprintln!("fatal error: {message}");
    set_default_camera();
    loop {
        clear_background(Color::from_rgba(10, 5, 12, 255));
        draw_text(
            "UNRUN FATAL ERROR",
            48.0,
            76.0,
            34.0,
            Color::from_rgba(255, 98, 140, 255),
        );
        draw_text(
            message,
            48.0,
            120.0,
            20.0,
            Color::from_rgba(235, 224, 230, 255),
        );
        draw_text(
            "See stderr for details. Close the window to exit.",
            48.0,
            158.0,
            18.0,
            Color::from_rgba(160, 150, 170, 255),
        );
        next_frame().await;
    }
}
