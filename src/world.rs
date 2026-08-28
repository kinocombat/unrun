use macroquad::prelude::{Rect, Vec2, vec2};

use crate::timeline::SnapshotState;

pub const WORLD_WIDTH: f32 = 1280.0;
pub const WORLD_HEIGHT: f32 = 720.0;
pub const FIXED_DT: f32 = 1.0 / 60.0;
pub const HISTORY_SECONDS: usize = 20;
pub const HISTORY_FRAMES: usize = HISTORY_SECONDS * 60;

pub const PLAYER_WIDTH: f32 = 30.0;
pub const PLAYER_HEIGHT: f32 = 48.0;
pub const FLOOR: Rect = Rect {
    x: 0.0,
    y: 620.0,
    w: WORLD_WIDTH,
    h: 100.0,
};
pub const FIRST_BLOCK: Rect = Rect {
    x: 300.0,
    y: 550.0,
    w: 84.0,
    h: 70.0,
};
pub const LAST_BLOCK: Rect = Rect {
    x: 970.0,
    y: 566.0,
    w: 92.0,
    h: 54.0,
};
pub const BEACON: Rect = Rect {
    x: 500.0,
    y: 548.0,
    w: 54.0,
    h: 72.0,
};
pub const GOAL: Rect = Rect {
    x: 1134.0,
    y: 496.0,
    w: 72.0,
    h: 124.0,
};
pub const DOOR_X: f32 = 790.0;

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

#[derive(Clone, Copy, Debug, Default)]
pub struct InputFrame {
    pub horizontal: f32,
    pub jump_pressed: bool,
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

impl Default for GameState {
    fn default() -> Self {
        Self {
            player: Player {
                position: vec2(82.0, FLOOR.y - PLAYER_HEIGHT),
                velocity: Vec2::ZERO,
                grounded: true,
                facing: 1.0,
                animation_phase: 0.0,
                coyote_timer: COYOTE_TIME,
                jump_buffer: 0.0,
            },
            completed: false,
        }
    }
}

/// State deliberately omitted from timeline snapshots.
///
/// The gate remembers events that happened in erased futures. This exception
/// to rewind is both a game rule and the stage's central puzzle mechanic.
#[derive(Clone, Debug, Default)]
pub struct FixedPointState {
    pub door_armed: bool,
    pub door_open: f32,
    pub door_latched: bool,
    pub rewound_frames: u64,
}

impl FixedPointState {
    pub fn arm(&mut self) {
        self.door_armed = true;
    }

    pub fn step_forward(&mut self, dt: f32) {
        if self.door_armed && !self.door_latched {
            self.door_open = (self.door_open - DOOR_FORWARD_DECAY * dt).max(0.0);
        }
    }

    pub fn step_rewind(&mut self, dt: f32) {
        self.rewound_frames += 1;
        if !self.door_armed || self.door_latched {
            return;
        }
        self.door_open = (self.door_open + dt / DOOR_REWIND_SECONDS).min(1.0);
        if self.door_open >= 1.0 {
            self.door_latched = true;
        }
    }

    pub fn door_rect(&self) -> Rect {
        let height = 380.0;
        let closed_top = FLOOR.y - height;
        let travel = 390.0 * self.door_open;
        Rect::new(DOOR_X, closed_top - travel, 42.0, height)
    }
}

impl GameState {
    pub fn step(&mut self, input: InputFrame, fixed: &mut FixedPointState, dt: f32) {
        if self.completed {
            return;
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
        }

        self.player.velocity.y = (self.player.velocity.y + GRAVITY * dt).min(MAX_FALL_SPEED);
        self.move_horizontally(fixed, dt);
        self.move_vertically(fixed, dt);

        if intersects(self.player.rect(), BEACON) {
            fixed.arm();
        }
        if intersects(self.player.rect(), GOAL) {
            self.completed = true;
            self.player.velocity = Vec2::ZERO;
        }
        self.player.animation_phase = (self.player.animation_phase
            + self.player.velocity.x.abs() * dt * 0.035)
            % std::f32::consts::TAU;
    }

    fn move_horizontally(&mut self, fixed: &FixedPointState, dt: f32) {
        self.player.position.x += self.player.velocity.x * dt;
        for solid in collision_solids(fixed) {
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

    fn move_vertically(&mut self, fixed: &FixedPointState, dt: f32) {
        self.player.position.y += self.player.velocity.y * dt;
        self.player.grounded = false;
        for solid in collision_solids(fixed) {
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
        let mut bytes = Vec::with_capacity(31);
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

pub fn static_solids() -> [Rect; 3] {
    [FLOOR, FIRST_BLOCK, LAST_BLOCK]
}

fn collision_solids(fixed: &FixedPointState) -> [Rect; 6] {
    [
        FLOOR,
        FIRST_BLOCK,
        LAST_BLOCK,
        Rect::new(-40.0, 0.0, 40.0, WORLD_HEIGHT),
        Rect::new(WORLD_WIDTH, 0.0, 40.0, WORLD_HEIGHT),
        fixed.door_rect(),
    ]
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

    fn auto_input(state: &GameState) -> InputFrame {
        let x = state.player.position.x;
        let needs_jump =
            state.player.grounded && ((230.0..410.0).contains(&x) || (900.0..1080.0).contains(&x));
        InputFrame {
            horizontal: 1.0,
            jump_pressed: needs_jump,
        }
    }

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
    fn closed_gate_blocks_a_forward_only_run() {
        let mut state = GameState::default();
        let mut fixed = FixedPointState::default();
        for _ in 0..600 {
            fixed.step_forward(FIXED_DT);
            let input = auto_input(&state);
            state.step(input, &mut fixed, FIXED_DT);
        }
        assert!(fixed.door_armed);
        assert!(!fixed.door_latched);
        assert!(state.player.position.x <= DOOR_X - PLAYER_WIDTH + 0.1);
        assert!(!state.completed);
    }

    #[test]
    fn rewind_opens_the_fixed_point_gate_and_stage_is_solvable() {
        let mut state = GameState::default();
        let mut fixed = FixedPointState::default();
        let mut timeline = Timeline::new(&state, HISTORY_FRAMES, 120).unwrap();

        for _ in 0..500 {
            fixed.step_forward(FIXED_DT);
            let input = auto_input(&state);
            state.step(input, &mut fixed, FIXED_DT);
            timeline.record(&state).unwrap();
            if fixed.door_armed && state.player.position.x > 700.0 {
                break;
            }
        }
        assert!(
            fixed.door_armed,
            "the scripted player did not reach the fixed point"
        );

        while !fixed.door_latched {
            assert!(timeline.rewind(&mut state).unwrap());
            fixed.step_rewind(FIXED_DT);
        }
        assert!(state.player.position.x < DOOR_X);

        for _ in 0..600 {
            fixed.step_forward(FIXED_DT);
            let input = auto_input(&state);
            state.step(input, &mut fixed, FIXED_DT);
            timeline.record(&state).unwrap();
            if state.completed {
                break;
            }
        }
        assert!(
            state.completed,
            "the scripted solution did not reach the exit"
        );
    }
}
