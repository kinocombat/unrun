use std::collections::VecDeque;

use macroquad::prelude::{Rect, Vec2};

use crate::editor::{EditorEvent, EditorInput, EditorState};
use crate::timeline::{Timeline, TimelineError};
use crate::world::{
    BONUS_PUZZLES, BONUS_TERMINALS, FIXED_DT, FixedPointState, GameState, HISTORY_FRAMES,
    InputFrame, STAGES, Stage, StepEvents, bonus_all_solved, is_bonus_stage, stage,
};

const MAX_FRAME_STEPS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameInput {
    pub horizontal: f32,
    pub jump_pressed: bool,
    pub rewind_down: bool,
    pub reset_pressed: bool,
    pub advance_pressed: bool,
    pub interact_pressed: bool,
    pub mute_pressed: bool,
    pub editor: EditorInput,
}

#[derive(Clone, Copy)]
pub(crate) struct Ghost {
    pub(crate) position: Vec2,
    pub(crate) facing: f32,
    pub(crate) life: f32,
}

pub struct Game {
    pub(crate) stage_index: usize,
    pub(crate) state: GameState,
    pub(crate) fixed: FixedPointState,
    pub(crate) timeline: Timeline,
    accumulator: f32,
    jump_queued: bool,
    pub(crate) rewind_active: bool,
    pub(crate) rewind_blocked_flash: f32,
    ghost_timer: usize,
    pub(crate) ghosts: VecDeque<Ghost>,
    pub(crate) bonus_solved: [bool; 3],
    pub(crate) bonus_notify_timer: f32,
    pub(crate) bonus_notify_text: String,
    editor: EditorState,
}

#[derive(Default)]
pub struct GameEvents {
    pub(crate) jumped: bool,
    pub(crate) fixed_point_activated: bool,
    pub(crate) gate_latched: bool,
    pub(crate) completed: bool,
    pub(crate) stage_changed: bool,
}

impl GameEvents {
    fn include_step(&mut self, events: StepEvents) {
        self.jumped |= events.jumped;
        self.fixed_point_activated |= events.fixed_point_activated;
        self.completed |= events.completed;
    }
}

impl Game {
    pub fn new(stage_index: usize) -> Result<Self, TimelineError> {
        let stage_index = stage_index % STAGES.len();
        let state = GameState::new(stage(stage_index));
        let timeline = Timeline::new(&state, HISTORY_FRAMES)?;
        Ok(Self {
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
            bonus_solved: [false; 3],
            bonus_notify_timer: 0.0,
            bonus_notify_text: String::new(),
            editor: EditorState::default(),
        })
    }

    pub fn open_editor(&mut self, terminal: usize) {
        self.editor.open(terminal);
    }

    pub fn stage_index(&self) -> usize {
        self.stage_index
    }

    pub fn is_rewinding(&self) -> bool {
        self.rewind_active
    }

    pub fn editor_is_open(&self) -> bool {
        self.editor.is_open()
    }

    pub(crate) fn editor_terminal(&self) -> usize {
        self.editor.terminal()
    }

    pub(crate) fn editor_text(&self) -> &str {
        self.editor.text()
    }

    pub(crate) fn editor_status(&self) -> &str {
        self.editor.status()
    }

    pub(crate) fn editor_status_is_error(&self) -> bool {
        self.editor.status_is_error()
    }

    pub(crate) fn current_stage(&self) -> &'static Stage {
        stage(self.stage_index)
    }

    pub(crate) fn is_last_stage(&self) -> bool {
        self.stage_index + 1 == STAGES.len()
    }

    fn reset(&mut self) -> Result<(), TimelineError> {
        *self = Self::new(self.stage_index)?;
        Ok(())
    }

    pub(crate) fn nearby_bonus_terminal(&self) -> Option<usize> {
        if !is_bonus_stage(self.stage_index) {
            return None;
        }
        let player = self.state.player.rect();
        let expanded =
            |rect: Rect| Rect::new(rect.x - 18.0, rect.y - 18.0, rect.w + 36.0, rect.h + 36.0);
        for (index, &terminal) in BONUS_TERMINALS.iter().enumerate() {
            if self.bonus_solved[index] {
                continue;
            }
            let area = expanded(terminal);
            if area.x < player.x + player.w
                && area.x + area.w > player.x
                && area.y < player.y + player.h
                && area.y + area.h > player.y
            {
                return Some(index);
            }
        }
        None
    }

    pub fn update(
        &mut self,
        input: &FrameInput,
        frame_dt: f32,
    ) -> Result<GameEvents, TimelineError> {
        let mut events = GameEvents::default();
        self.bonus_notify_timer = (self.bonus_notify_timer - frame_dt).max(0.0);

        if self.editor.is_open() {
            if let EditorEvent::Submitted { accepted: true } = self.editor.update(&input.editor) {
                let terminal = self.editor.terminal();
                self.bonus_solved[terminal] = true;
                if bonus_all_solved(&self.bonus_solved) && !self.fixed.door_armed {
                    self.fixed.arm();
                    self.bonus_notify_text = "ANOMALY ACCEPTED - FIXED POINT ARMED".to_owned();
                    self.bonus_notify_timer = 3.0;
                } else {
                    self.bonus_notify_text = format!(
                        "TERMINAL {}/3 - {}",
                        self.bonus_solved.iter().filter(|&&value| value).count(),
                        BONUS_PUZZLES[terminal].success
                    );
                    self.bonus_notify_timer = 2.2;
                }
                events.fixed_point_activated = true;
                if bonus_all_solved(&self.bonus_solved) {
                    events.gate_latched = true;
                }
            }
            self.accumulator = 0.0;
            self.rewind_active = false;
            return Ok(events);
        }

        if is_bonus_stage(self.stage_index) {
            if let Some(terminal) = self.nearby_bonus_terminal() {
                if input.interact_pressed {
                    self.open_editor(terminal);
                    return Ok(events);
                }
            }
        }

        if input.reset_pressed {
            self.reset()?;
            events.stage_changed = true;
            return Ok(events);
        }
        if self.state.completed && input.advance_pressed {
            *self = Self::new((self.stage_index + 1) % STAGES.len())?;
            events.stage_changed = true;
            return Ok(events);
        }

        let frame_dt = frame_dt.min(0.1);
        self.rewind_blocked_flash = (self.rewind_blocked_flash - frame_dt * 2.5).max(0.0);
        for ghost in &mut self.ghosts {
            ghost.life -= frame_dt * 1.9;
        }
        while self.ghosts.front().is_some_and(|ghost| ghost.life <= 0.0) {
            self.ghosts.pop_front();
        }

        let rewind_requested = input.rewind_down;
        self.jump_queued |= input.jump_pressed;

        if self.state.completed && !rewind_requested {
            self.accumulator = 0.0;
            self.rewind_active = false;
            return Ok(events);
        }

        let horizontal = input.horizontal.clamp(-1.0, 1.0);
        self.accumulator += frame_dt;
        self.rewind_active = false;

        let mut steps = 0;
        let current_stage = stage(self.stage_index);
        while self.accumulator >= FIXED_DT && steps < MAX_FRAME_STEPS {
            if rewind_requested {
                let old_player = self.state.player;
                match self.timeline.rewind(&mut self.state)? {
                    true => {
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
                    false => {
                        self.rewind_blocked_flash = 1.0;
                    }
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
                self.timeline.record(&self.state)?;
            }
            self.accumulator -= FIXED_DT;
            steps += 1;
        }
        if steps == MAX_FRAME_STEPS {
            self.accumulator = 0.0;
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::SnapshotState;
    use crate::world::BONUS_STAGE_INDEX;

    #[test]
    fn reset_restores_the_same_stage_initial_state() {
        let stage_index = 2;
        let mut game = Game::new(stage_index).unwrap();
        game.state.player.position.x += 100.0;
        game.fixed.arm();

        let events = game
            .update(
                &FrameInput {
                    reset_pressed: true,
                    ..FrameInput::default()
                },
                0.0,
            )
            .unwrap();

        assert!(events.stage_changed);
        assert_eq!(game.stage_index, stage_index);
        assert_eq!(
            game.state.encode_snapshot(),
            GameState::new(stage(stage_index)).encode_snapshot()
        );
        assert!(!game.fixed.door_armed);
        assert_eq!(game.timeline.available_frames(), 0);
    }

    #[test]
    fn completed_stage_advances_to_the_next_stage() {
        let mut game = Game::new(1).unwrap();
        game.state.completed = true;

        let events = game
            .update(
                &FrameInput {
                    advance_pressed: true,
                    ..FrameInput::default()
                },
                0.0,
            )
            .unwrap();

        assert!(events.stage_changed);
        assert_eq!(game.stage_index, 2);
        assert!(!game.state.completed);
    }

    #[test]
    fn completed_final_stage_wraps_to_stage_zero() {
        let mut game = Game::new(STAGES.len() - 1).unwrap();
        game.state.completed = true;

        game.update(
            &FrameInput {
                advance_pressed: true,
                ..FrameInput::default()
            },
            0.0,
        )
        .unwrap();

        assert_eq!(game.stage_index, 0);
    }

    #[test]
    fn rewind_with_empty_history_preserves_state_and_flashes_blocked() {
        let mut game = Game::new(0).unwrap();
        let before = game.state.encode_snapshot();

        game.update(
            &FrameInput {
                rewind_down: true,
                ..FrameInput::default()
            },
            FIXED_DT,
        )
        .unwrap();

        assert_eq!(game.state.encode_snapshot(), before);
        assert_eq!(game.timeline.available_frames(), 0);
        assert!(!game.rewind_active);
        assert_eq!(game.rewind_blocked_flash, 1.0);
    }

    #[test]
    fn accepted_bonus_submission_fires_success_events_only_once() {
        const ANSWERS: [&str; 3] = [
            "fn main() {
            let timeline = String::from(\"unrun\");
            let past = timeline.clone();
            println!(\"{}\", timeline);
        }",
            "enum Gate { Closed, Open }
        fn gate_state(fixed: bool, rewound: u32) -> Gate {
            if fixed && rewound >= 75 { Gate::Open } else { Gate::Closed }
        }",
            "fn sum_even(nums: &[i32]) -> i32 {
        nums.iter().filter(|n| *n % 2 == 0).sum()
    }",
        ];
        let submit = FrameInput {
            editor: EditorInput {
                submit: true,
                ..EditorInput::default()
            },
            ..FrameInput::default()
        };
        let close = FrameInput {
            editor: EditorInput {
                close: true,
                ..EditorInput::default()
            },
            ..FrameInput::default()
        };

        let mut game = Game::new(BONUS_STAGE_INDEX).unwrap();
        for (terminal, answer) in ANSWERS.iter().enumerate() {
            game.open_editor(terminal);
            game.editor.set_text_for_test(answer);

            let first = game.update(&submit, 0.0).unwrap();
            assert!(first.fixed_point_activated);
            assert_eq!(first.gate_latched, terminal == ANSWERS.len() - 1);

            // エディタが開いたまま再 submit しても成功イベントは再発火しない
            let again = game.update(&submit, 0.0).unwrap();
            assert!(!again.fixed_point_activated);
            assert!(!again.gate_latched);

            game.update(&close, 0.0).unwrap();
        }
        assert!(game.bonus_solved.iter().all(|&solved| solved));
    }
}
