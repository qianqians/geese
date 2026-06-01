//! 基于 [rodio](https://github.com/RustAudio/rodio) 的真实音频后端。
//!
//! - 解码：默认启用 `wav` + `vorbis`，按需扩展 `mp3`/`flac`/`mp4`。
//! - 设备：`OutputStream::try_default()` 自动选择系统默认输出设备；headless 或
//!   无声卡环境会返回 [`AudioError::BackendUnavailable`]，方便单测软跳过。
//! - 单实例播放：每个 [`Sound`] 持一个独占 `rodio::Sink`，pause/play/stop/volume
//!   一一对应。
//! - 3D 空间化：当前版本只做软件 distance attenuation 接口占位；后续可升级到
//!   `rodio::SpatialSink` 做真正的双耳定位。

use super::*;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/// rodio 后端：持有 OutputStream（保活）+ Handle（共享给 Sink）。
pub struct RodioBackend {
    // OutputStream 必须保活，drop 即所有声音停止。它不 Send/Sync，因此 trait 不
    // 强制 Send + Sync。
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    sources: HashMap<SourceId, Arc<Vec<u8>>>,
    next_id: u64,
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
        })
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

        Ok(Arc::new(RodioSound { sink }))
    }
}

/// 单个 rodio 播放实例（独占一个 Sink）。
pub struct RodioSound {
    sink: rodio::Sink,
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
        self.sink.set_volume(volume.clamp(0.0, 8.0));
    }
    fn set_pitch(&self, pitch: f32) {
        self.sink.set_speed(pitch.max(0.01));
    }
    fn set_position(&self, _pos: SoundPosition) {
        // 占位：未来改用 SpatialSink 或在此根据 Listener 软件衰减。
    }
    fn is_playing(&self) -> bool {
        !self.sink.is_paused() && !self.sink.empty()
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
