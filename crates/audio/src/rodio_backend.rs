//! 基于 [rodio](https://github.com/RustAudio/rodio) 的真实音频后端。
//!
//! - 解码：默认启用 `wav` + `vorbis`，按需扩展 `mp3`/`flac`/`mp4`。
//! - 设备：`OutputStream::try_default()` 自动选择系统默认输出设备;headless 或
//!   无声卡环境会返回 [`AudioError::BackendUnavailable`]，方便单测软跳过。
//! - 单实例播放：每个 [`Sound`] 持一个独占 `rodio::Sink`，pause/play/stop/volume
//!   一一对应。
//! - 3D 空间化：当前版本只做软件 distance attenuation 接口占位;后续可升级到
//!   `rodio::SpatialSink` 做真正的双耳定位。

use super::*;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::Mutex;

/// 默认距离衰减系数（越大衰减越快）。
pub const DEFAULT_ROLLOFF_FACTOR: f32 = 0.1;

/// 默认最大可听距离，超过此距离衰减系数为 0。
pub const DEFAULT_MAX_DISTANCE: f32 = 100.0;

/// 共享的 3D 衰减参数（由 RodioBackend 持有并注入到每个 RodioSound）。
#[derive(Debug, Clone, Copy)]
pub struct AttenuationParams {
    pub rolloff_factor: f32,
    pub max_distance: f32,
}

impl Default for AttenuationParams {
    fn default() -> Self {
        Self {
            rolloff_factor: DEFAULT_ROLLOFF_FACTOR,
            max_distance: DEFAULT_MAX_DISTANCE,
        }
    }
}

/// 计算距离衰减系数。
///
/// - 反距离模型：`atten = 1.0 / (1.0 + dist * rolloff)`
/// - 当距离超过 `max_distance` 时直接返回 `0.0`
/// - 当距离为 0 时返回 `1.0`
pub fn compute_attenuation(
    listener_pos: [f32; 3],
    sound_pos: [f32; 3],
    rolloff: f32,
    max_distance: f32,
) -> f32 {
    let dx = listener_pos[0] - sound_pos[0];
    let dy = listener_pos[1] - sound_pos[1];
    let dz = listener_pos[2] - sound_pos[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    if dist >= max_distance {
        return 0.0;
    }
    1.0 / (1.0 + dist * rolloff)
}

/// rodio 后端：持有 OutputStream（保活）+ Handle（共享给 Sink）。
pub struct RodioBackend {
    // OutputStream 必须保活，drop 即所有声音停止。它不 Send/Sync，因此 trait 不
    // 强制 Send + Sync。
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    sources: HashMap<SourceId, Arc<Vec<u8>>>,
    next_id: u64,
    /// 共享监听者（注入到每个 RodioSound）
    listener: Arc<Mutex<Listener>>,
    /// 共享衰减参数（注入到每个 RodioSound）
    attenuation: Arc<Mutex<AttenuationParams>>,
}

impl RodioBackend {
    pub fn try_new() -> Result<Self, AudioError> {
        let (stream, handle) = rodio::OutputStream::try_default()
            .map_err(|_| AudioError::BackendUnavailable)?;
        Ok(Self {
            _stream: stream,
            handle,
            sources: HashMap::new(),
            next_id: 0,
            listener: Arc::new(Mutex::new(Listener::default())),
            attenuation: Arc::new(Mutex::new(AttenuationParams::default())),
        })
    }

    /// 更新衰减参数（影响所有活跃 RodioSound 的距离衰减计算）。
    pub fn set_attenuation(&self, rolloff_factor: f32, max_distance: f32) {
        if let Ok(mut guard) = self.attenuation.lock() {
            guard.rolloff_factor = rolloff_factor;
            guard.max_distance = max_distance;
        }
    }

    /// 更新监听者位置（影响所有活跃 RodioSound 的距离衰减）。
    pub fn update_listener(&self, listener: Listener) {
        if let Ok(mut guard) = self.listener.lock() {
            *guard = listener;
        }
    }


}

impl AudioBackend for RodioBackend {
    fn load_bytes(&mut self, bytes: &[u8]) -> Result<SourceId, AudioError> {
        // 预先 Decode 一次以验证字节合法，避免后续 spawn 时才暴露 DecodeFailed。
        let probe = Cursor::new(bytes.to_vec());
        rodio::Decoder::new(probe)
            .map_err(|e| AudioError::DecodeFailed(e.to_string()))?;

        self.next_id += 1;
        let id = SourceId(self.next_id);
        self.sources.insert(id, Arc::new(bytes.to_vec()));
        Ok(id)
    }

    fn unload(&mut self, id: SourceId) {
        self.sources.remove(&id);
    }

    fn update_listener(&mut self, listener: Listener) {
        RodioBackend::update_listener(self, listener);
    }

    fn spawn(&self, id: SourceId, config: SoundConfig) -> Result<Arc<dyn Sound>, AudioError> {
        let bytes = self
            .sources
            .get(&id)
            .ok_or(AudioError::SourceNotFound(id))?
            .clone();
        let cursor = Cursor::new((*bytes).clone());
        let decoder = rodio::Decoder::new(cursor)
            .map_err(|e| AudioError::DecodeFailed(e.to_string()))?;

        let sink = rodio::Sink::try_new(&self.handle)
            .map_err(|e| AudioError::DecodeFailed(format!("create sink failed: {e}")))?;

        if config.looping {
            use rodio::Source;
            sink.append(decoder.repeat_infinite());
        } else {
            sink.append(decoder);
        }

        sink.set_volume(config.volume.clamp(0.0, 8.0));
        sink.set_speed(config.pitch.max(0.01));
        // 默认暂停，由 AudioSystem::play_2d/3d 显式 play()，与 NullBackend 行为对齐。
        sink.pause();

        let sound = RodioSound {
            sink,
            position: Mutex::new(config.position.unwrap_or_default()),
            listener: self.listener.clone(),
            attenuation: self.attenuation.clone(),
            base_volume_milli: AtomicU32::new(
                (config.volume.clamp(0.0, 8.0) * 1000.0) as u32,
            ),
            spatial: Mutex::new(SpatialParams::default()),
        };
        Ok(Arc::new(sound))
    }
}

/// 单个 rodio 播放实例（独占一个 Sink）。
pub struct RodioSound {
    sink: rodio::Sink,
    /// 声源位置
    position: Mutex<SoundPosition>,
    /// 共享监听者（由 RodioBackend 注入）
    listener: Arc<Mutex<Listener>>,
    /// 共享衰减参数（由 RodioBackend 注入）
    attenuation: Arc<Mutex<AttenuationParams>>,
    /// 原始音量（千分位整数），衰减基于此值
    base_volume_milli: AtomicU32,
    /// 双耳空间化参数缓存（最近一次计算的左右耳增益）
    spatial: Mutex<SpatialParams>,
}

impl RodioSound {
    /// 返回当前 base volume（f32）。
    fn base_volume(&self) -> f32 {
        self.base_volume_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }

    /// 计算当前有效音量 = base_volume * distance_attenuation * spatial_gain
    fn effective_volume(&self) -> f32 {
        let atten = self.current_attenuation();
        let spatial_gain = self.spatial.lock().ok()
            .map(|sp| (sp.left_gain + sp.right_gain) * 0.5)
            .unwrap_or(1.0);
        self.base_volume() * atten * spatial_gain
    }
}

impl Sound for RodioSound {
    fn play(&self) {
        self.sink.play();
    }
    fn pause(&self) {
        self.sink.pause();
    }
    fn stop(&self) {
        self.sink.stop();
    }
    fn set_volume(&self, volume: f32) {
        // 存储 base volume，重新计算有效音量（含距离衰减）
        let clamped = volume.clamp(0.0, 8.0);
        self.base_volume_milli
            .store((clamped * 1000.0) as u32, Ordering::Relaxed);
        let effective = clamped * self.current_attenuation();
        self.sink.set_volume(effective.clamp(0.0, 8.0));
    }
    fn set_pitch(&self, pitch: f32) {
        self.sink.set_speed(pitch.max(0.01));
    }
    fn set_position(&self, pos: SoundPosition) {
        // 存储 position，重新计算距离衰减 + 双耳空间化，更新 sink volume
        if let Ok(mut guard) = self.position.lock() {
            *guard = pos;
        }
        // 计算双耳空间化参数
        let listener_guard = self.listener.lock().ok();
        let listener_pos = listener_guard.as_ref().map(|g| g.position).unwrap_or([0.0; 3]);
        let listener_fwd = listener_guard.as_ref().map(|g| g.forward).unwrap_or([0.0, 0.0, -1.0]);
        let sp = compute_spatial_params(listener_pos, listener_fwd, pos.position);
        if let Ok(mut sg) = self.spatial.lock() {
            *sg = sp;
        }
        // 有效音量 = base * 距离衰减 * 双耳平均增益
        let effective = self.effective_volume();
        self.sink.set_volume(effective.clamp(0.0, 8.0));
    }
    fn is_playing(&self) -> bool {
        !self.sink.is_paused() && !self.sink.empty()
    }
}

impl RodioSound {
    /// 当前距离衰减系数。
    fn current_attenuation(&self) -> f32 {
        let pos = self.position.lock().ok().map(|g| g.position).unwrap_or([0.0; 3]);
        let listener_pos = self
            .listener
            .lock()
            .ok()
            .map(|g| g.position)
            .unwrap_or([0.0; 3]);
        let (rolloff, max_dist) = self
            .attenuation
            .lock()
            .ok()
            .map(|g| (g.rolloff_factor, g.max_distance))
            .unwrap_or((DEFAULT_ROLLOFF_FACTOR, DEFAULT_MAX_DISTANCE));
        compute_attenuation(listener_pos, pos, rolloff, max_dist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一段 `ms` 毫秒的 16-bit mono 静音 WAV 字节流，用于 rodio Decoder 测试。
    fn silent_wav(ms: u32) -> Vec<u8> {
        let sample_rate: u32 = 44100;
        let num_samples = sample_rate * ms / 1000;
        let data_size: u32 = num_samples * 2; // 16-bit mono
        let chunk_size: u32 = 36 + data_size;
        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&chunk_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    #[test]
    fn rodio_backend_inits_or_skips_on_headless() {
        // CI / headless 环境无声卡时 BackendUnavailable，应当静默跳过。
        match RodioBackend::try_new() {
            Ok(_) => {}
            Err(AudioError::BackendUnavailable) => return,
            Err(e) => panic!("unexpected init error: {e}"),
        }
    }

    #[test]
    fn rodio_backend_decodes_wav_and_spawns_sound() {
        let Ok(mut backend) = RodioBackend::try_new() else {
            return; // headless 跳过
        };
        let wav = silent_wav(20);
        let id = backend.load_bytes(&wav).unwrap();
        let s = backend
            .spawn(id, SoundConfig { volume: 0.0, ..Default::default() })
            .unwrap();
        // 应初始暂停
        assert!(!s.is_playing());
        s.play();
        // play 后可能立即播完（静音 20ms 也许还在队列），不强校验 is_playing。
        s.stop();
    }

    #[test]
    fn rodio_backend_rejects_garbage_bytes_on_load() {
        let Ok(mut backend) = RodioBackend::try_new() else {
            return;
        };
        let err = backend.load_bytes(b"not-a-real-audio-file");
        assert!(matches!(err, Err(AudioError::DecodeFailed(_))));
    }

    #[test]
    fn rodio_unloaded_source_cannot_spawn() {
        let Ok(mut backend) = RodioBackend::try_new() else {
            return;
        };
        let id = backend.load_bytes(&silent_wav(5)).unwrap();
        backend.unload(id);
        let err = backend.spawn(id, SoundConfig::default());
        assert!(matches!(err, Err(AudioError::SourceNotFound(_))));
    }
}
