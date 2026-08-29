use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, set_sound_volume, stop_sound,
};

use crate::game::GameEvents;
use crate::sound::{
    fixed_point_wav, gate_latched_wav, jump_wav, rewind_loop_wav, stage_clear_wav,
    uk_garage_loop_wav,
};

pub struct AudioSystem {
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
    pub async fn load() -> Result<Self, macroquad::Error> {
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

    pub fn update(&mut self, events: &GameEvents, rewinding: bool, mute_pressed: bool) {
        if mute_pressed {
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

    pub fn is_muted(&self) -> bool {
        self.muted
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
