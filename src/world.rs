use macroquad::prelude::{Rect, Vec2, vec2};

use crate::timeline::SnapshotState;

pub const WORLD_WIDTH: f32 = 1280.0;
pub const WORLD_HEIGHT: f32 = 720.0;
pub const FIXED_DT: f32 = 1.0 / 60.0;
pub const HISTORY_SECONDS: usize = 20;
pub const HISTORY_FRAMES: usize = HISTORY_SECONDS * 60;
pub const PLAYER_WIDTH: f32 = 30.0;
pub const PLAYER_HEIGHT: f32 = 48.0;

const MOVE_ACCELERATION: f32 = 1800.0;
const GROUND_FRICTION: f32 = 2200.0;
const MAX_RUN_SPEED: f32 = 260.0;
const GRAVITY: f32 = 1550.0;
const MAX_FALL_SPEED: f32 = 900.0;
const JUMP_SPEED: f32 = 570.0;
const COYOTE_TIME: f32 = 0.10;
const JUMP_BUFFER_TIME: f32 = 0.12;
const DOOR_REWIND_SECONDS: f32 = 1.25;
const DOOR_FORWARD_DECAY: f32 = 0.36;

const STAGE_ONE_SOLIDS: [Rect; 3] = [
    Rect {
        x: 0.0,
        y: 620.0,
        w: WORLD_WIDTH,
        h: 100.0,
    },
    Rect {
        x: 300.0,
        y: 550.0,
        w: 84.0,
        h: 70.0,
    },
    Rect {
        x: 970.0,
        y: 566.0,
        w: 92.0,
        h: 54.0,
    },
];

const STAGE_TWO_SOLIDS: [Rect; 4] = [
    Rect {
        x: 0.0,
        y: 650.0,
        w: WORLD_WIDTH,
        h: 70.0,
    },
    Rect {
        x: 0.0,
        y: 420.0,
        w: 360.0,
        h: 30.0,
    },
    Rect {
        x: 510.0,
        y: 420.0,
        w: 770.0,
        h: 30.0,
    },
    Rect {
        x: 950.0,
        y: 370.0,
        w: 82.0,
        h: 50.0,
    },
];

const STAGE_THREE_SOLIDS: [Rect; 3] = [
    Rect {
        x: 0.0,
        y: 620.0,
        w: WORLD_WIDTH,
        h: 100.0,
    },
    Rect {
        x: 760.0,
        y: 530.0,
        w: 86.0,
        h: 90.0,
    },
    Rect {
        x: 210.0,
        y: 550.0,
        w: 84.0,
        h: 70.0,
    },
];

#[derive(Clone, Copy, Debug)]
pub struct Stage {
    pub name: &'static str,
    pub subtitle: &'static str,
    pub spawn: [f32; 2],
    pub spawn_facing: f32,
    pub solids: &'static [Rect],
    pub base_floor_y: f32,
    pub beacon: Rect,
    pub goal: Rect,
    pub door_x: f32,
    pub door_floor_y: f32,
    pub jump_hint: [f32; 2],
}

impl Stage {
    pub fn spawn_position(self) -> Vec2 {
        vec2(self.spawn[0], self.spawn[1])
    }
}

pub const STAGES: [Stage; 3] = [
    Stage {
        name: "FIRST CONTACT",
        subtitle: "TEACH THE GATE TO REMEMBER",
        spawn: [82.0, 572.0],
        spawn_facing: 1.0,
        solids: &STAGE_ONE_SOLIDS,
        base_floor_y: 620.0,
        beacon: Rect {
            x: 500.0,
            y: 548.0,
            w: 54.0,
            h: 72.0,
        },
        goal: Rect {
            x: 1134.0,
            y: 496.0,
            w: 72.0,
            h: 124.0,
        },
        door_x: 790.0,
        door_floor_y: 620.0,
        jump_hint: [300.0, 526.0],
    },
    Stage {
        name: "THE DROP",
        subtitle: "FALL FOR THE SIGNAL / ERASE THE FALL",
        spawn: [82.0, 372.0],
        spawn_facing: 1.0,
        solids: &STAGE_TWO_SOLIDS,
        base_floor_y: 650.0,
        beacon: Rect {
            x: 1050.0,
            y: 578.0,
            w: 54.0,
            h: 72.0,
        },
        goal: Rect {
            x: 1144.0,
            y: 296.0,
            w: 72.0,
            h: 124.0,
        },
        door_x: 740.0,
        door_floor_y: 420.0,
        jump_hint: [315.0, 388.0],
    },
    Stage {
        name: "B-SIDE",
        subtitle: "THE SIGNAL AND EXIT FACE OPPOSITE WAYS",
        spawn: [590.0, 572.0],
        spawn_facing: 1.0,
        solids: &STAGE_THREE_SOLIDS,
        base_floor_y: 620.0,
        beacon: Rect {
            x: 1120.0,
            y: 548.0,
            w: 54.0,
            h: 72.0,
        },
        goal: Rect {
            x: 52.0,
            y: 496.0,
            w: 72.0,
            h: 124.0,
        },
        door_x: 390.0,
        door_floor_y: 620.0,
        jump_hint: [760.0, 506.0],
    },
];

pub fn stage(index: usize) -> &'static Stage {
    &STAGES[index % STAGES.len()]
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InputFrame {
    pub horizontal: f32,
    pub jump_pressed: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StepEvents {
    pub jumped: bool,
    pub fixed_point_activated: bool,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub position: Vec2,
    pub velocity: Vec2,
    pub grounded: bool,
    pub facing: f32,
    pub animation_phase: f32,
    coyote_timer: f32,
    jump_buffer: f32,
}

impl Player {
    pub fn rect(self) -> Rect {
        Rect::new(
            self.position.x,
            self.position.y,
            PLAYER_WIDTH,
            PLAYER_HEIGHT,
        )
    }
}

#[derive(Clone, Debug)]
pub struct GameState {
    pub player: Player,
    pub completed: bool,
}

impl GameState {
    pub fn new(stage: &Stage) -> Self {
        Self {
            player: Player {
                position: stage.spawn_position(),
                velocity: Vec2::ZERO,
                grounded: true,
                facing: stage.spawn_facing,
                animation_phase: 0.0,
                coyote_timer: COYOTE_TIME,
                jump_buffer: 0.0,
            },
            completed: false,
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new(stage(0))
    }
}

/// State deliberately omitted from timeline snapshots.
#[derive(Clone, Debug, Default)]
pub struct FixedPointState {
    pub door_armed: bool,
    pub door_open: f32,
    pub door_latched: bool,
    pub rewound_frames: u64,
}

impl FixedPointState {
    pub fn arm(&mut self) -> bool {
        let activated = !self.door_armed;
        self.door_armed = true;
        activated
    }

    pub fn step_forward(&mut self, dt: f32) {
        if self.door_armed && !self.door_latched {
            self.door_open = (self.door_open - DOOR_FORWARD_DECAY * dt).max(0.0);
        }
    }

    pub fn step_rewind(&mut self, dt: f32) -> bool {
        self.rewound_frames += 1;
        if !self.door_armed || self.door_latched {
            return false;
        }
        self.door_open = (self.door_open + dt / DOOR_REWIND_SECONDS).min(1.0);
        if self.door_open >= 1.0 {
            self.door_latched = true;
            return true;
        }
        false
    }

    pub fn door_rect(&self, stage: &Stage) -> Rect {
        let height = 380.0;
        let closed_top = stage.door_floor_y - height;
        let travel = 390.0 * self.door_open;
        Rect::new(stage.door_x, closed_top - travel, 42.0, height)
    }
}

impl GameState {
    pub fn step(
        &mut self,
        input: InputFrame,
        fixed: &mut FixedPointState,
        stage: &Stage,
        dt: f32,
    ) -> StepEvents {
        let mut events = StepEvents::default();
        if self.completed {
            return events;
        }

        let horizontal = input.horizontal.clamp(-1.0, 1.0);
        if horizontal.abs() > 0.01 {
            self.player.velocity.x = approach(
                self.player.velocity.x,
                horizontal * MAX_RUN_SPEED,
                MOVE_ACCELERATION * dt,
            );
            self.player.facing = horizontal.signum();
        } else {
            self.player.velocity.x = approach(self.player.velocity.x, 0.0, GROUND_FRICTION * dt);
        }

        self.player.jump_buffer = (self.player.jump_buffer - dt).max(0.0);
        if input.jump_pressed {
            self.player.jump_buffer = JUMP_BUFFER_TIME;
        }
        if self.player.grounded {
            self.player.coyote_timer = COYOTE_TIME;
        } else {
            self.player.coyote_timer = (self.player.coyote_timer - dt).max(0.0);
        }
        if self.player.jump_buffer > 0.0 && self.player.coyote_timer > 0.0 {
            self.player.velocity.y = -JUMP_SPEED;
            self.player.grounded = false;
            self.player.jump_buffer = 0.0;
            self.player.coyote_timer = 0.0;
            events.jumped = true;
        }

        self.player.velocity.y = (self.player.velocity.y + GRAVITY * dt).min(MAX_FALL_SPEED);
        self.move_horizontally(fixed, stage, dt);
        self.move_vertically(fixed, stage, dt);

        if intersects(self.player.rect(), stage.beacon) {
            events.fixed_point_activated = fixed.arm();
        }
        if intersects(self.player.rect(), stage.goal) {
            self.completed = true;
            self.player.velocity = Vec2::ZERO;
            events.completed = true;
        }
        self.player.animation_phase = (self.player.animation_phase
            + self.player.velocity.x.abs() * dt * 0.035)
            % std::f32::consts::TAU;
        events
    }

    fn move_horizontally(&mut self, fixed: &FixedPointState, stage: &Stage, dt: f32) {
        self.player.position.x += self.player.velocity.x * dt;
        for solid in collision_solids(stage, fixed) {
            let player = self.player.rect();
            if !intersects(player, solid) {
                continue;
            }
            if self.player.velocity.x > 0.0 {
                self.player.position.x = solid.x - PLAYER_WIDTH;
            } else if self.player.velocity.x < 0.0 {
                self.player.position.x = solid.x + solid.w;
            }
            self.player.velocity.x = 0.0;
        }
    }

    fn move_vertically(&mut self, fixed: &FixedPointState, stage: &Stage, dt: f32) {
        self.player.position.y += self.player.velocity.y * dt;
        self.player.grounded = false;
        for solid in collision_solids(stage, fixed) {
            let player = self.player.rect();
            if !intersects(player, solid) {
                continue;
            }
            if self.player.velocity.y > 0.0 {
                self.player.position.y = solid.y - PLAYER_HEIGHT;
                self.player.grounded = true;
            } else if self.player.velocity.y < 0.0 {
                self.player.position.y = solid.y + solid.h;
            }
            self.player.velocity.y = 0.0;
        }
    }
}

impl SnapshotState for GameState {
    fn encode_snapshot(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(34);
        for value in [
            self.player.position.x,
            self.player.position.y,
            self.player.velocity.x,
            self.player.velocity.y,
            self.player.facing,
            self.player.animation_phase,
            self.player.coyote_timer,
            self.player.jump_buffer,
        ] {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes.push(u8::from(self.player.grounded));
        bytes.push(u8::from(self.completed));
        bytes
    }

    fn restore_snapshot(&mut self, bytes: &[u8]) -> Result<(), String> {
        const FLOAT_COUNT: usize = 8;
        const ENCODED_LEN: usize = FLOAT_COUNT * 4 + 2;
        if bytes.len() != ENCODED_LEN {
            return Err(format!(
                "expected {ENCODED_LEN} bytes, found {}",
                bytes.len()
            ));
        }

        let mut values = [0.0; FLOAT_COUNT];
        for (index, value) in values.iter_mut().enumerate() {
            let offset = index * 4;
            let bits = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            *value = f32::from_bits(bits);
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err("snapshot contains a non-finite number".into());
        }
        let bool_offset = FLOAT_COUNT * 4;
        if bytes[bool_offset] > 1 || bytes[bool_offset + 1] > 1 {
            return Err("snapshot contains an invalid boolean".into());
        }

        self.player.position = vec2(values[0], values[1]);
        self.player.velocity = vec2(values[2], values[3]);
        self.player.facing = values[4];
        self.player.animation_phase = values[5];
        self.player.coyote_timer = values[6];
        self.player.jump_buffer = values[7];
        self.player.grounded = bytes[bool_offset] != 0;
        self.completed = bytes[bool_offset + 1] != 0;
        Ok(())
    }
}

fn collision_solids<'a>(
    stage: &'a Stage,
    fixed: &'a FixedPointState,
) -> impl Iterator<Item = Rect> + 'a {
    stage.solids.iter().copied().chain([
        Rect::new(-40.0, 0.0, 40.0, WORLD_HEIGHT),
        Rect::new(WORLD_WIDTH, 0.0, 40.0, WORLD_HEIGHT),
        fixed.door_rect(stage),
    ])
}

pub fn intersects(left: Rect, right: Rect) -> bool {
    left.x < right.x + right.w
        && left.x + left.w > right.x
        && left.y < right.y + right.h
        && left.y + left.h > right.y
}

fn approach(current: f32, target: f32, amount: f32) -> f32 {
    if current < target {
        (current + amount).min(target)
    } else {
        (current - amount).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Timeline;

    #[test]
    fn snapshot_round_trip_is_exact() {
        let mut state = GameState::default();
        state.player.position = vec2(412.25, 118.5);
        state.player.velocity = vec2(-93.0, 41.0);
        state.player.grounded = false;
        let expected = state.clone();
        let bytes = state.encode_snapshot();
        state = GameState::default();
        state.restore_snapshot(&bytes).unwrap();
        assert_eq!(state.encode_snapshot(), expected.encode_snapshot());
    }

    #[test]
    fn first_stage_still_blocks_a_forward_only_run() {
        let stage = stage(0);
        let mut state = GameState::new(stage);
        let mut fixed = FixedPointState::default();
        for _ in 0..600 {
            fixed.step_forward(FIXED_DT);
            let input = stage_one_input(&state, 1.0);
            state.step(input, &mut fixed, stage, FIXED_DT);
        }
        assert!(fixed.door_armed);
        assert!(!fixed.door_latched);
        assert!(state.player.position.x <= stage.door_x - PLAYER_WIDTH + 0.1);
        assert!(!state.completed);
    }

    #[test]
    fn every_stage_has_a_scripted_rewind_solution() {
        for stage_index in 0..STAGES.len() {
            assert!(
                solve_stage(stage_index),
                "stage {stage_index} was not solvable"
            );
        }
    }

    fn solve_stage(stage_index: usize) -> bool {
        let stage = stage(stage_index);
        let mut state = GameState::new(stage);
        let mut fixed = FixedPointState::default();
        let mut timeline = Timeline::new(&state, HISTORY_FRAMES, 120).unwrap();

        for _ in 0..900 {
            fixed.step_forward(FIXED_DT);
            let input = match stage_index {
                0 => stage_one_input(&state, 1.0),
                1 => InputFrame {
                    horizontal: 1.0,
                    jump_pressed: false,
                },
                2 => stage_three_input(&state, 1.0),
                _ => unreachable!(),
            };
            state.step(input, &mut fixed, stage, FIXED_DT);
            timeline.record(&state).unwrap();
            if fixed.door_armed {
                break;
            }
        }
        if !fixed.door_armed {
            return false;
        }

        for _ in 0..900 {
            if !timeline.rewind(&mut state).unwrap() {
                return false;
            }
            fixed.step_rewind(FIXED_DT);
            let ready = match stage_index {
                0 => fixed.door_latched && state.player.position.x < 470.0,
                1 => {
                    fixed.door_latched
                        && state.player.position.y < stage.door_floor_y
                        && state.player.position.x < 330.0
                }
                2 => fixed.door_latched && state.player.position.x < 650.0,
                _ => unreachable!(),
            };
            if ready {
                break;
            }
        }
        if !fixed.door_latched {
            return false;
        }

        for _ in 0..1_200 {
            fixed.step_forward(FIXED_DT);
            let input = match stage_index {
                0 => stage_one_input(&state, 1.0),
                1 => stage_two_finish_input(&state),
                2 => stage_three_input(&state, -1.0),
                _ => unreachable!(),
            };
            state.step(input, &mut fixed, stage, FIXED_DT);
            timeline.record(&state).unwrap();
            if state.completed {
                return true;
            }
        }
        false
    }

    fn stage_one_input(state: &GameState, direction: f32) -> InputFrame {
        let x = state.player.position.x;
        let needs_jump =
            state.player.grounded && ((230.0..410.0).contains(&x) || (900.0..1080.0).contains(&x));
        InputFrame {
            horizontal: direction,
            jump_pressed: needs_jump,
        }
    }

    fn stage_two_finish_input(state: &GameState) -> InputFrame {
        let x = state.player.position.x;
        InputFrame {
            horizontal: 1.0,
            jump_pressed: state.player.grounded
                && ((270.0..430.0).contains(&x) || (880.0..1050.0).contains(&x)),
        }
    }

    fn stage_three_input(state: &GameState, direction: f32) -> InputFrame {
        let x = state.player.position.x;
        let needs_jump = if direction > 0.0 {
            (690.0..870.0).contains(&x)
        } else {
            (270.0..470.0).contains(&x)
        };
        InputFrame {
            horizontal: direction,
            jump_pressed: state.player.grounded && needs_jump,
        }
    }
}
