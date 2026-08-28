use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;

const HASH_BYTES: usize = 16;
const GC_INTERVAL: usize = 120;

/// A state that can be captured by the rewind timeline.
///
/// Implementations must use one canonical encoding: equal states must produce
/// equal bytes on every platform where saved history is consumed.
pub trait SnapshotState {
    fn encode_snapshot(&self) -> Vec<u8>;
    fn restore_snapshot(&mut self, bytes: &[u8]) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BlobId([u8; HASH_BYTES]);

impl BlobId {
    fn for_blob(kind: BlobKind, bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[kind as u8]);
        hasher.update(bytes);
        let hash = hasher.finalize();
        let mut short = [0; HASH_BYTES];
        short.copy_from_slice(&hash.as_bytes()[..HASH_BYTES]);
        Self(short)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum BlobKind {
    Checkpoint = 1,
    Delta = 2,
}

#[derive(Clone)]
struct Blob {
    kind: BlobKind,
    bytes: Arc<[u8]>,
}

#[derive(Default)]
struct ContentStore {
    blobs: HashMap<BlobId, Blob>,
    deduplicated_writes: usize,
}

impl ContentStore {
    fn insert(&mut self, kind: BlobKind, bytes: Vec<u8>) -> Result<BlobId, TimelineError> {
        let id = BlobId::for_blob(kind, &bytes);
        if let Some(existing) = self.blobs.get(&id) {
            if existing.kind != kind || existing.bytes.as_ref() != bytes.as_slice() {
                return Err(TimelineError::HashCollision);
            }
            self.deduplicated_writes += 1;
            return Ok(id);
        }

        self.blobs.insert(
            id,
            Blob {
                kind,
                bytes: Arc::from(bytes),
            },
        );
        Ok(id)
    }

    fn get(&self, id: BlobId, expected: BlobKind) -> Result<Arc<[u8]>, TimelineError> {
        let blob = self.blobs.get(&id).ok_or(TimelineError::MissingBlob)?;
        if blob.kind != expected || BlobId::for_blob(blob.kind, &blob.bytes) != id {
            return Err(TimelineError::CorruptBlob);
        }
        Ok(Arc::clone(&blob.bytes))
    }

    fn retain(&mut self, live: &HashSet<BlobId>) {
        self.blobs.retain(|id, _| live.contains(id));
    }

    fn byte_len(&self) -> usize {
        self.blobs.values().map(|blob| blob.bytes.len()).sum()
    }
}

#[derive(Clone, Debug)]
struct FrameDelta {
    delta: BlobId,
    checkpoint: Option<BlobId>,
    logical_bytes: usize,
}

/// Runtime measurements for the in-memory content-addressed history.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimelineStats {
    pub rewindable_frames: usize,
    pub capacity_frames: usize,
    pub blob_count: usize,
    pub stored_payload_bytes: usize,
    pub logical_snapshot_bytes: usize,
    pub deduplicated_writes: usize,
}

impl TimelineStats {
    pub fn payload_saving_percent(self) -> f32 {
        if self.logical_snapshot_bytes == 0 {
            return 0.0;
        }
        let ratio = self.stored_payload_bytes as f32 / self.logical_snapshot_bytes as f32;
        ((1.0 - ratio) * 100.0).clamp(0.0, 99.9)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum TimelineError {
    CorruptBlob,
    HashCollision,
    InvalidDelta(&'static str),
    MissingBlob,
    Snapshot(String),
    StateTooLarge,
}

impl fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CorruptBlob => write!(formatter, "timeline blob failed content verification"),
            Self::HashCollision => write!(formatter, "content hash collision detected"),
            Self::InvalidDelta(reason) => write!(formatter, "invalid timeline delta: {reason}"),
            Self::MissingBlob => write!(formatter, "timeline references a missing blob"),
            Self::Snapshot(reason) => write!(formatter, "could not restore snapshot: {reason}"),
            Self::StateTooLarge => write!(formatter, "snapshot exceeds the delta format limit"),
        }
    }
}

impl std::error::Error for TimelineError {}

/// A bounded, frame-addressable rewind history.
///
/// Full checkpoints and XOR deltas share one content-addressed store. Repeated
/// idle frames therefore cost one shared empty delta instead of full copies.
pub struct Timeline {
    store: ContentStore,
    frames: VecDeque<FrameDelta>,
    current: Vec<u8>,
    initial_checkpoint: Option<BlobId>,
    capacity: usize,
    checkpoint_interval: usize,
    frames_recorded: usize,
    discarded_since_gc: usize,
}

impl Timeline {
    pub fn new<S: SnapshotState>(
        state: &S,
        capacity: usize,
        checkpoint_interval: usize,
    ) -> Result<Self, TimelineError> {
        let current = state.encode_snapshot();
        if current.len() > u16::MAX as usize {
            return Err(TimelineError::StateTooLarge);
        }

        let mut store = ContentStore::default();
        let initial_checkpoint = store.insert(BlobKind::Checkpoint, current.clone())?;
        Ok(Self {
            store,
            frames: VecDeque::with_capacity(capacity.min(4096)),
            current,
            initial_checkpoint: Some(initial_checkpoint),
            capacity,
            checkpoint_interval: checkpoint_interval.max(1),
            frames_recorded: 0,
            discarded_since_gc: 0,
        })
    }

    pub fn record<S: SnapshotState>(&mut self, state: &S) -> Result<(), TimelineError> {
        let next = state.encode_snapshot();
        if next.len() > u16::MAX as usize {
            return Err(TimelineError::StateTooLarge);
        }

        let encoded_delta = encode_delta(&self.current, &next)?;
        let delta = self.store.insert(BlobKind::Delta, encoded_delta)?;
        self.frames_recorded += 1;
        let checkpoint = if self.frames_recorded % self.checkpoint_interval == 0 {
            Some(self.store.insert(BlobKind::Checkpoint, next.clone())?)
        } else {
            None
        };

        self.frames.push_back(FrameDelta {
            delta,
            checkpoint,
            logical_bytes: next.len(),
        });
        self.current = next;

        if self.frames.len() > self.capacity {
            self.frames.pop_front();
            self.initial_checkpoint = None;
            self.discarded_since_gc += 1;
        }
        self.collect_garbage_if_needed();
        Ok(())
    }

    pub fn rewind<S: SnapshotState>(&mut self, state: &mut S) -> Result<bool, TimelineError> {
        let Some(frame) = self.frames.back().cloned() else {
            return Ok(false);
        };
        let delta = self.store.get(frame.delta, BlobKind::Delta)?;
        let previous = apply_delta_backwards(&self.current, &delta)?;
        state
            .restore_snapshot(&previous)
            .map_err(TimelineError::Snapshot)?;

        self.frames.pop_back();
        self.current = previous;
        self.discarded_since_gc += 1;
        self.collect_garbage_if_needed();
        Ok(true)
    }

    pub fn can_rewind(&self) -> bool {
        !self.frames.is_empty()
    }

    pub fn available_frames(&self) -> usize {
        self.frames.len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn stats(&self) -> TimelineStats {
        TimelineStats {
            rewindable_frames: self.frames.len(),
            capacity_frames: self.capacity,
            blob_count: self.store.blobs.len(),
            stored_payload_bytes: self.store.byte_len(),
            logical_snapshot_bytes: self.current.len()
                + self
                    .frames
                    .iter()
                    .map(|frame| frame.logical_bytes)
                    .sum::<usize>(),
            deduplicated_writes: self.store.deduplicated_writes,
        }
    }

    pub fn collect_garbage(&mut self) {
        let mut live = HashSet::with_capacity(self.frames.len() * 2 + 1);
        if let Some(initial) = self.initial_checkpoint {
            live.insert(initial);
        }
        for frame in &self.frames {
            live.insert(frame.delta);
            if let Some(checkpoint) = frame.checkpoint {
                live.insert(checkpoint);
            }
        }
        self.store.retain(&live);
        self.discarded_since_gc = 0;
    }

    fn collect_garbage_if_needed(&mut self) {
        if self.discarded_since_gc >= GC_INTERVAL {
            self.collect_garbage();
        }
    }
}

// Equal-sized snapshots use a bit mask followed by only the non-zero XOR
// bytes. XOR is symmetric, so the same blob can move one frame backward.
fn encode_delta(before: &[u8], after: &[u8]) -> Result<Vec<u8>, TimelineError> {
    if before.len() == after.len() {
        if before.len() > u16::MAX as usize {
            return Err(TimelineError::StateTooLarge);
        }
        let mask_len = before.len().div_ceil(8);
        let mut mask = vec![0_u8; mask_len];
        let mut changed = Vec::new();
        for (index, (&old, &new)) in before.iter().zip(after).enumerate() {
            let xor = old ^ new;
            if xor != 0 {
                mask[index / 8] |= 1 << (index % 8);
                changed.push(xor);
            }
        }

        let mut encoded = Vec::with_capacity(3 + mask.len() + changed.len());
        encoded.push(0);
        encoded.extend_from_slice(&(before.len() as u16).to_le_bytes());
        encoded.extend_from_slice(&mask);
        encoded.extend_from_slice(&changed);
        return Ok(encoded);
    }

    let before_len = u32::try_from(before.len()).map_err(|_| TimelineError::StateTooLarge)?;
    let after_len = u32::try_from(after.len()).map_err(|_| TimelineError::StateTooLarge)?;
    let mut encoded = Vec::with_capacity(9 + before.len() + after.len());
    encoded.push(1);
    encoded.extend_from_slice(&before_len.to_le_bytes());
    encoded.extend_from_slice(&after_len.to_le_bytes());
    encoded.extend_from_slice(before);
    encoded.extend_from_slice(after);
    Ok(encoded)
}

fn apply_delta_backwards(current: &[u8], delta: &[u8]) -> Result<Vec<u8>, TimelineError> {
    let Some((&mode, body)) = delta.split_first() else {
        return Err(TimelineError::InvalidDelta("empty blob"));
    };
    match mode {
        0 => apply_xor_delta(current, body),
        1 => apply_replacement_delta(current, body),
        _ => Err(TimelineError::InvalidDelta("unknown encoding")),
    }
}

fn apply_xor_delta(current: &[u8], delta: &[u8]) -> Result<Vec<u8>, TimelineError> {
    if delta.len() < 2 {
        return Err(TimelineError::InvalidDelta("missing snapshot length"));
    }
    let state_len = u16::from_le_bytes([delta[0], delta[1]]) as usize;
    if current.len() != state_len {
        return Err(TimelineError::InvalidDelta("snapshot length mismatch"));
    }
    let mask_len = state_len.div_ceil(8);
    if delta.len() < 2 + mask_len {
        return Err(TimelineError::InvalidDelta("truncated change mask"));
    }
    let mask = &delta[2..2 + mask_len];
    let values = &delta[2 + mask_len..];
    let expected_values = mask.iter().map(|byte| byte.count_ones() as usize).sum();
    if values.len() != expected_values {
        return Err(TimelineError::InvalidDelta(
            "change payload length mismatch",
        ));
    }

    let mut restored = current.to_vec();
    let mut value_index = 0;
    for index in 0..state_len {
        if mask[index / 8] & (1 << (index % 8)) != 0 {
            restored[index] ^= values[value_index];
            value_index += 1;
        }
    }
    Ok(restored)
}

fn apply_replacement_delta(current: &[u8], delta: &[u8]) -> Result<Vec<u8>, TimelineError> {
    if delta.len() < 8 {
        return Err(TimelineError::InvalidDelta("truncated replacement header"));
    }
    let before_len = u32::from_le_bytes(delta[0..4].try_into().unwrap()) as usize;
    let after_len = u32::from_le_bytes(delta[4..8].try_into().unwrap()) as usize;
    if delta.len() != 8 + before_len + after_len {
        return Err(TimelineError::InvalidDelta("replacement length mismatch"));
    }
    let before = &delta[8..8 + before_len];
    let after = &delta[8 + before_len..];
    if current != after {
        return Err(TimelineError::InvalidDelta(
            "replacement applied to wrong state",
        ));
    }
    Ok(before.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct TestState {
        left: u32,
        right: u32,
    }

    impl SnapshotState for TestState {
        fn encode_snapshot(&self) -> Vec<u8> {
            [self.left.to_le_bytes(), self.right.to_le_bytes()].concat()
        }

        fn restore_snapshot(&mut self, bytes: &[u8]) -> Result<(), String> {
            if bytes.len() != 8 {
                return Err("wrong test snapshot length".into());
            }
            self.left = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            self.right = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            Ok(())
        }
    }

    #[test]
    fn rewinds_every_recorded_frame() {
        let mut state = TestState { left: 1, right: 2 };
        let mut timeline = Timeline::new(&state, 60, 10).unwrap();
        for value in 2..=12 {
            state.left = value;
            timeline.record(&state).unwrap();
        }

        for expected in (1..=11).rev() {
            assert!(timeline.rewind(&mut state).unwrap());
            assert_eq!(state.left, expected);
            assert_eq!(state.right, 2);
        }
        assert!(!timeline.rewind(&mut state).unwrap());
    }

    #[test]
    fn repeated_idle_frames_share_one_delta_blob() {
        let state = TestState { left: 7, right: 9 };
        let mut timeline = Timeline::new(&state, 600, 120).unwrap();
        for _ in 0..100 {
            timeline.record(&state).unwrap();
        }

        let stats = timeline.stats();
        assert_eq!(stats.rewindable_frames, 100);
        assert!(
            stats.blob_count <= 2,
            "{} blobs were stored",
            stats.blob_count
        );
        assert!(stats.deduplicated_writes >= 99);
    }

    #[test]
    fn history_capacity_is_a_hard_rewind_limit() {
        let mut state = TestState { left: 0, right: 0 };
        let mut timeline = Timeline::new(&state, 3, 2).unwrap();
        for value in 1..=5 {
            state.left = value;
            timeline.record(&state).unwrap();
        }

        assert_eq!(timeline.available_frames(), 3);
        for expected in [4, 3, 2] {
            assert!(timeline.rewind(&mut state).unwrap());
            assert_eq!(state.left, expected);
        }
        assert!(!timeline.rewind(&mut state).unwrap());
    }

    #[test]
    fn variable_length_snapshots_have_a_safe_fallback() {
        let before = b"short";
        let after = b"a longer state";
        let delta = encode_delta(before, after).unwrap();
        assert_eq!(apply_delta_backwards(after, &delta).unwrap(), before);
        assert!(apply_delta_backwards(b"wrong state", &delta).is_err());
    }
}
