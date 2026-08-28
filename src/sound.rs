use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 44_100;
pub const UKG_BPM: f32 = 132.0;
const BARS: usize = 4;
const STEPS_PER_BAR: usize = 16;

struct StereoBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl StereoBuffer {
    fn seconds(duration: f32) -> Self {
        let frames = (duration * SAMPLE_RATE as f32).round() as usize;
        Self {
            left: vec![0.0; frames],
            right: vec![0.0; frames],
        }
    }

    fn add(&mut self, frame: usize, sample: f32, pan: f32) {
        if frame >= self.left.len() {
            return;
        }
        let pan = pan.clamp(-1.0, 1.0);
        self.left[frame] += sample * (1.0 - pan) * 0.5;
        self.right[frame] += sample * (1.0 + pan) * 0.5;
    }

    fn wav(self) -> Vec<u8> {
        encode_wav(&self.left, &self.right)
    }
}

/// Generates a four-bar, 132 BPM UK garage loop with no external assets.
pub fn uk_garage_loop_wav() -> Vec<u8> {
    let step_seconds = 60.0 / UKG_BPM / 4.0;
    let duration = BARS as f32 * STEPS_PER_BAR as f32 * step_seconds;
    let mut mix = StereoBuffer::seconds(duration);

    let kicks: [&[usize]; BARS] = [&[0, 7, 10], &[0, 6, 11], &[0, 7, 10, 15], &[0, 6, 10]];
    let bass_notes: [[f32; 5]; BARS] = [
        [43.65, 43.65, 51.91, 65.41, 51.91],
        [43.65, 51.91, 58.27, 65.41, 51.91],
        [43.65, 43.65, 77.78, 65.41, 51.91],
        [43.65, 58.27, 51.91, 65.41, 43.65],
    ];
    let bass_steps = [0, 3, 7, 10, 14];

    for bar in 0..BARS {
        let offset = bar * STEPS_PER_BAR;
        for &step in kicks[bar] {
            add_kick(&mut mix, step_frame(offset + step, step_seconds), 0.86);
        }
        for step in [4, 12] {
            add_snare(
                &mut mix,
                step_frame(offset + step, step_seconds),
                0.78,
                (bar * 31 + step) as u32,
            );
        }
        for step in 0..STEPS_PER_BAR {
            if step % 2 == 1 || matches!(step, 2 | 6 | 10 | 14) {
                let open = matches!(step, 6 | 14);
                let pan = if step % 4 == 1 { -0.42 } else { 0.42 };
                add_hat(
                    &mut mix,
                    step_frame(offset + step, step_seconds),
                    if open { 0.19 } else { 0.055 },
                    if open { 0.24 } else { 0.16 },
                    pan,
                    (offset + step) as u32,
                );
            }
        }
        for step in [2, 6, 10, 14] {
            add_chord_stab(
                &mut mix,
                step_frame(offset + step, step_seconds),
                if step % 8 == 2 { -0.22 } else { 0.22 },
            );
        }
        for (index, step) in bass_steps.into_iter().enumerate() {
            add_bass(
                &mut mix,
                step_frame(offset + step, step_seconds),
                bass_notes[bar][index],
                step_seconds * if index == 4 { 1.4 } else { 2.3 },
                0.52,
            );
        }
    }

    master(mix).wav()
}

pub fn jump_wav() -> Vec<u8> {
    let duration = 0.18;
    let mut mix = StereoBuffer::seconds(duration);
    let mut phase = 0.0;
    for frame in 0..mix.left.len() {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let progress = t / duration;
        let frequency = 260.0 + 470.0 * progress.powf(0.65);
        phase += TAU * frequency / SAMPLE_RATE as f32;
        let envelope = (1.0 - progress).powi(2) * (t * 90.0).min(1.0);
        mix.add(frame, phase.sin() * envelope * 0.46, 0.0);
    }
    master(mix).wav()
}

pub fn fixed_point_wav() -> Vec<u8> {
    let mut mix = StereoBuffer::seconds(0.72);
    for (index, frequency) in [349.23, 415.30, 523.25, 622.25].into_iter().enumerate() {
        add_tone(
            &mut mix,
            (index as f32 * 0.055 * SAMPLE_RATE as f32) as usize,
            frequency,
            0.55,
            0.27,
            -0.45 + index as f32 * 0.3,
        );
    }
    master(mix).wav()
}

pub fn gate_latched_wav() -> Vec<u8> {
    let mut mix = StereoBuffer::seconds(0.9);
    add_bass(&mut mix, 0, 43.65, 0.8, 0.9);
    for (index, frequency) in [174.61, 261.63, 349.23].into_iter().enumerate() {
        add_tone(&mut mix, index * 1_500, frequency, 0.62, 0.24, 0.0);
    }
    master(mix).wav()
}

pub fn stage_clear_wav() -> Vec<u8> {
    let mut mix = StereoBuffer::seconds(1.25);
    for (index, frequency) in [174.61, 207.65, 261.63, 311.13, 392.0, 523.25]
        .into_iter()
        .enumerate()
    {
        add_tone(
            &mut mix,
            (index as f32 * 0.105 * SAMPLE_RATE as f32) as usize,
            frequency,
            0.5,
            0.25,
            -0.5 + index as f32 * 0.2,
        );
    }
    master(mix).wav()
}

pub fn rewind_loop_wav() -> Vec<u8> {
    let duration = 1.0;
    let mut mix = StereoBuffer::seconds(duration);
    for frame in 0..mix.left.len() {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let pulse = 0.55 + 0.45 * (TAU * 4.0 * t).sin();
        let left = (TAU * 83.0 * t).sin() + 0.45 * (TAU * 167.0 * t).sin();
        let right = (TAU * 79.0 * t).sin() + 0.45 * (TAU * 173.0 * t).sin();
        mix.left[frame] += left * pulse * 0.095;
        mix.right[frame] += right * pulse * 0.095;
    }
    master(mix).wav()
}

fn step_frame(step: usize, step_seconds: f32) -> usize {
    let swing_delay = if step % 2 == 1 {
        step_seconds * 0.18
    } else {
        0.0
    };
    ((step as f32 * step_seconds + swing_delay) * SAMPLE_RATE as f32) as usize
}

fn add_kick(mix: &mut StereoBuffer, start: usize, level: f32) {
    let frames = (0.42 * SAMPLE_RATE as f32) as usize;
    let mut phase = 0.0;
    for index in 0..frames {
        let t = index as f32 / SAMPLE_RATE as f32;
        let frequency = 46.0 + 115.0 * (-t * 24.0).exp();
        phase += TAU * frequency / SAMPLE_RATE as f32;
        let body = phase.sin() * (-t * 11.0).exp();
        let click = noise(index as u32, 17) * (-t * 95.0).exp() * 0.22;
        mix.add(start + index, (body + click) * level, 0.0);
    }
}

fn add_snare(mix: &mut StereoBuffer, start: usize, level: f32, seed: u32) {
    let frames = (0.26 * SAMPLE_RATE as f32) as usize;
    let mut previous = 0.0;
    for index in 0..frames {
        let t = index as f32 / SAMPLE_RATE as f32;
        let raw = noise(index as u32, seed);
        let bright = raw - previous * 0.72;
        previous = raw;
        let envelope = (-t * 17.0).exp();
        let body = (TAU * 184.0 * t).sin() * (-t * 22.0).exp() * 0.34;
        mix.add(
            start + index,
            (bright * 0.46 + body) * envelope * level,
            0.08,
        );
    }
}

fn add_hat(mix: &mut StereoBuffer, start: usize, duration: f32, level: f32, pan: f32, seed: u32) {
    let frames = (duration * SAMPLE_RATE as f32) as usize;
    let mut previous = 0.0;
    for index in 0..frames {
        let t = index as f32 / SAMPLE_RATE as f32;
        let raw = noise(index as u32, seed.wrapping_mul(97).wrapping_add(5));
        let metallic = raw - previous;
        previous = raw;
        let envelope = (-t * if duration > 0.1 { 17.0 } else { 58.0 }).exp();
        mix.add(start + index, metallic * envelope * level, pan);
    }
}

fn add_bass(mix: &mut StereoBuffer, start: usize, frequency: f32, duration: f32, level: f32) {
    let frames = (duration * SAMPLE_RATE as f32) as usize;
    for index in 0..frames {
        let t = index as f32 / SAMPLE_RATE as f32;
        let attack = (t * 70.0).min(1.0);
        let release = (1.0 - t / duration).clamp(0.0, 1.0).powf(0.7);
        let fundamental = (TAU * frequency * t).sin();
        let harmonic = (TAU * frequency * 2.0 * t).sin() * 0.16;
        mix.add(
            start + index,
            (fundamental + harmonic) * attack * release * level,
            0.0,
        );
    }
}

fn add_chord_stab(mix: &mut StereoBuffer, start: usize, pan: f32) {
    for (index, frequency) in [174.61, 207.65, 261.63, 311.13, 392.0]
        .into_iter()
        .enumerate()
    {
        add_tone(
            mix,
            start,
            frequency,
            0.24,
            0.085,
            pan + (index as f32 - 2.0) * 0.08,
        );
    }
}

fn add_tone(
    mix: &mut StereoBuffer,
    start: usize,
    frequency: f32,
    duration: f32,
    level: f32,
    pan: f32,
) {
    let frames = (duration * SAMPLE_RATE as f32) as usize;
    for index in 0..frames {
        let t = index as f32 / SAMPLE_RATE as f32;
        let attack = (t * 90.0).min(1.0);
        let release = (1.0 - t / duration).clamp(0.0, 1.0).powi(2);
        let sine = (TAU * frequency * t).sin();
        let soft_square = ((TAU * frequency * t).sin() * 2.2).tanh();
        mix.add(
            start + index,
            (sine * 0.68 + soft_square * 0.32) * attack * release * level,
            pan,
        );
    }
}

fn noise(index: u32, seed: u32) -> f32 {
    let mut value = index.wrapping_add(seed.wrapping_mul(0x9e37_79b9));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    (value as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn master(mut mix: StereoBuffer) -> StereoBuffer {
    for (left, right) in mix.left.iter_mut().zip(&mut mix.right) {
        *left = (*left * 0.78).tanh() * 0.88;
        *right = (*right * 0.78).tanh() * 0.88;
    }
    mix
}

fn encode_wav(left: &[f32], right: &[f32]) -> Vec<u8> {
    assert_eq!(left.len(), right.len());
    let data_bytes = (left.len() * 2 * size_of::<i16>()) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for (&left, &right) in left.iter().zip(right) {
        wav.extend_from_slice(&pcm(left).to_le_bytes());
        wav.extend_from_slice(&pcm(right).to_le_bytes());
    }
    wav
}

fn pcm(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ukg_loop_is_valid_stereo_pcm_with_expected_duration() {
        let wav = uk_garage_loop_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().unwrap()),
            SAMPLE_RATE
        );
        let data_bytes = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        let expected_seconds = BARS as f32 * 4.0 * 60.0 / UKG_BPM;
        let actual_seconds = data_bytes as f32 / (SAMPLE_RATE as f32 * 4.0);
        assert!((actual_seconds - expected_seconds).abs() < 0.001);
        assert!(peak(&wav) > 4_000);
    }

    #[test]
    fn generated_effects_are_nonempty_valid_wav_files() {
        for wav in [
            jump_wav(),
            fixed_point_wav(),
            gate_latched_wav(),
            stage_clear_wav(),
            rewind_loop_wav(),
        ] {
            assert_eq!(&wav[0..4], b"RIFF");
            assert!(wav.len() > 1_000);
            assert!(peak(&wav) > 1_000);
        }
    }

    fn peak(wav: &[u8]) -> i16 {
        wav[44..]
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]).unsigned_abs())
            .max()
            .unwrap_or(0) as i16
    }
}
