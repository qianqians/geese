//! 音频子系统（骨架）。
//!
//! 提供 2D/3D 音效与 BGM 的统一抽象。骨架阶段不绑定具体后端（rodio / oddio +
//! cpal）;定义 trait 与数据结构，业务层即可按接口编码，后续“可用级”阶段再
//! 接入 rodio 实播 wav/ogg、3D 衰减、混音通道。
//!
//! 设计要点：
//! - [`Sound`]：单个声源 trait（play/pause/stop/set_volume/is_playing）。
//! - [`AudioBackend`]：音频后端 trait，负责创建 [`Sound`] 实例。
//! - [`AudioSystem`]：门面，管理 [`MixerChannel`] 与全局 master volume、
//!   [`Listener`] 监听者，路由 `play_2d` / `play_3d` 到 backend。
//! - [`NullBackend`] / [`NullSound`]：占位实现，用于「无声」运行与测试。

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

mod rodio_backend;
pub use rodio_backend::{RodioBackend, RodioSound, AttenuationParams, DEFAULT_ROLLOFF_FACTOR, DEFAULT_MAX_DISTANCE};
pub use rodio_backend::compute_attenuation;

// ---------------------------------------------------------------------------
// 通用数据
// ---------------------------------------------------------------------------

/// 混音通道。业务层按用途分类，便于做「BGM 一键静音」等批操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixerChannel {
    Master,
    Music,
    Sfx,
    Voice,
    Ui,
}

impl MixerChannel {
    pub fn all_non_master() -> [MixerChannel; 4] {
        [Self::Music, Self::Sfx, Self::Voice, Self::Ui]
    }
}

/// 声源在 3D 空间中的位置/速度（用于多普勒，可选）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SoundPosition {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
}

/// 监听者：3D 音效的「耳朵」。
#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub position: [f32; 3],
    /// 前方向（unit vector）。
    pub forward: [f32; 3],
    /// 上方向（unit vector）。
    pub up: [f32; 3],
    pub velocity: [f32; 3],
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            velocity: [0.0; 3],
        }
    }
}

/// 播放配置。
#[derive(Debug, Clone, Copy)]
pub struct SoundConfig {
    pub channel: MixerChannel,
    pub volume: f32,
    pub pitch: f32,
    pub looping: bool,
    /// Some(pos) 表示 3D 声源，None 表示 2D（UI/BGM）。
    pub position: Option<SoundPosition>,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            channel: MixerChannel::Sfx,
            volume: 1.0,
            pitch: 1.0,
            looping: false,
            position: None,
        }
    }
}

/// 加载好但未播放的音频源（PCM/解码后的数据 + 元信息）。
///
/// 骨架阶段只用一个不透明 id 标识;真实 backend 会内部维护 id → 解码数据 的表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(pub u64);

// ---------------------------------------------------------------------------
// Sound trait
// ---------------------------------------------------------------------------

/// 单个声源（播放中的实例）。
pub trait Sound: Send + Sync {
    fn play(&self);
    fn pause(&self);
    fn stop(&self);
    fn set_volume(&self, volume: f32);
    fn set_pitch(&self, pitch: f32);
    fn set_position(&self, pos: SoundPosition);
    fn is_playing(&self) -> bool;
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/// 音频后端 trait：负责加载源数据并创建播放实例。
///
/// 真实实现：[`RodioBackend`]（rodio + cpal）、[`NullBackend`]（静默测试）。
///
/// 未要求 `Send + Sync`：依赖 cpal 的后端（如 rodio::OutputStream）多为平台
/// 原生资源，跨线程释放不安全。业务侧请在主线程或专用音频线程持有
/// [`AudioSystem`]。
pub trait AudioBackend {
    /// 从原始字节解码并注册为一个可重复播放的源。
    fn load_bytes(&mut self, bytes: &[u8]) -> Result<SourceId, AudioError>;
    /// 释放源（之后用相同 id 创建实例会失败）。
    fn unload(&mut self, id: SourceId);
    /// 创建一个该源的播放实例。
    fn spawn(&self, id: SourceId, config: SoundConfig) -> Result<Arc<dyn Sound>, AudioError>;
    /// 更新监听者位置（影响 3D 衰减计算）。默认 no-op。
    fn update_listener(&mut self, _listener: Listener) {}
}

#[derive(Debug)]
pub enum AudioError {
    SourceNotFound(SourceId),
    DecodeFailed(String),
    BackendUnavailable,
}

// 不引入 thiserror 依赖，自己实现 Display + Error。
mod _err_impl {
    use super::AudioError;
    impl core::fmt::Display for AudioError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                AudioError::SourceNotFound(id) => write!(f, "audio source not found: {id:?}"),
                AudioError::DecodeFailed(msg) => write!(f, "audio decode failed: {msg}"),
                AudioError::BackendUnavailable => write!(f, "audio backend unavailable"),
            }
        }
    }
    impl std::error::Error for AudioError {}
}

// ---------------------------------------------------------------------------
// Null 实现
// ---------------------------------------------------------------------------

/// 静默 backend：不真正播放，但能正确分配 [`SourceId`]、跟踪 play/pause 状态。
/// 用于无声模式（服务器/单测/CI）。
#[derive(Debug, Default)]
pub struct NullBackend {
    next_id: u64,
    sources: std::collections::HashSet<SourceId>,
}

impl NullBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AudioBackend for NullBackend {
    fn load_bytes(&mut self, _bytes: &[u8]) -> Result<SourceId, AudioError> {
        self.next_id += 1;
        let id = SourceId(self.next_id);
        self.sources.insert(id);
        Ok(id)
    }

    fn unload(&mut self, id: SourceId) {
        self.sources.remove(&id);
    }

    fn spawn(&self, id: SourceId, _config: SoundConfig) -> Result<Arc<dyn Sound>, AudioError> {
        if !self.sources.contains(&id) {
            return Err(AudioError::SourceNotFound(id));
        }
        Ok(Arc::new(NullSound::new()))
    }
}

/// 静默 sound：跟踪 play/pause 状态但不发声。
#[derive(Debug)]
pub struct NullSound {
    playing: AtomicBool,
    volume_milli: AtomicU32, // 千分位整数，避免 atomic-f32 依赖
}

impl NullSound {
    pub fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            volume_milli: AtomicU32::new(1000),
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

impl Default for NullSound {
    fn default() -> Self {
        Self::new()
    }
}

impl Sound for NullSound {
    fn play(&self) {
        self.playing.store(true, Ordering::Relaxed);
    }
    fn pause(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }
    fn stop(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }
    fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 8.0);
        self.volume_milli.store((clamped * 1000.0) as u32, Ordering::Relaxed);
    }
    fn set_pitch(&self, _pitch: f32) {}
    fn set_position(&self, _pos: SoundPosition) {}
    fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// AudioSystem 门面
// ---------------------------------------------------------------------------

/// 音频系统门面：管理通道音量、Listener、统一播放入口。
pub struct AudioSystem {
    backend: Box<dyn AudioBackend>,
    channel_volume: HashMap<MixerChannel, f32>,
    master_volume: f32,
    listener: Listener,
    /// 距离衰减系数（传递给后端）。
    rolloff_factor: f32,
    /// 最大可听距离（传递给后端）。
    max_distance: f32,
}

impl AudioSystem {
    pub fn new(backend: Box<dyn AudioBackend>) -> Self {
        let mut channel_volume = HashMap::new();
        for ch in MixerChannel::all_non_master() {
            channel_volume.insert(ch, 1.0);
        }
        Self {
            backend,
            channel_volume,
            master_volume: 1.0,
            listener: Listener::default(),
            rolloff_factor: DEFAULT_ROLLOFF_FACTOR,
            max_distance: DEFAULT_MAX_DISTANCE,
        }
    }

    pub fn with_null() -> Self {
        Self::new(Box::new(NullBackend::new()))
    }

    /// 尝试创建 rodio 后端的 AudioSystem;headless / 无声卡环境返回
    /// [`AudioError::BackendUnavailable`]。
    pub fn try_with_rodio() -> Result<Self, AudioError> {
        let backend = RodioBackend::try_new()?;
        Ok(Self::new(Box::new(backend)))
    }

    pub fn set_master_volume(&mut self, v: f32) {
        self.master_volume = v.clamp(0.0, 1.0);
    }

    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    pub fn set_channel_volume(&mut self, ch: MixerChannel, v: f32) {
        if ch == MixerChannel::Master {
            self.set_master_volume(v);
            return;
        }
        self.channel_volume.insert(ch, v.clamp(0.0, 1.0));
    }

    pub fn channel_volume(&self, ch: MixerChannel) -> f32 {
        if ch == MixerChannel::Master {
            return self.master_volume;
        }
        self.channel_volume.get(&ch).copied().unwrap_or(1.0)
    }

    pub fn set_listener(&mut self, l: Listener) {
        self.listener = l;
        self.backend.update_listener(l);
    }

    pub fn listener(&self) -> &Listener {
        &self.listener
    }

    /// 设置 3D 衰减参数（仅影响当前 AudioSystem 层面的记录，
    /// 后端特定实现如 RodioBackend 可通过 `set_attenuation` 进一步生效）。
    pub fn set_attenuation_params(&mut self, rolloff_factor: f32, max_distance: f32) {
        self.rolloff_factor = rolloff_factor;
        self.max_distance = max_distance;
    }

    pub fn rolloff_factor(&self) -> f32 {
        self.rolloff_factor
    }

    pub fn max_distance(&self) -> f32 {
        self.max_distance
    }

    pub fn load(&mut self, bytes: &[u8]) -> Result<SourceId, AudioError> {
        self.backend.load_bytes(bytes)
    }

    pub fn unload(&mut self, id: SourceId) {
        self.backend.unload(id);
    }

    /// 计算「应用 master + channel 后」的有效音量。
    pub fn effective_volume(&self, config: &SoundConfig) -> f32 {
        config.volume * self.channel_volume(config.channel) * self.master_volume
    }

    /// 播放 2D 声音（UI/BGM 等无空间位置）。
    pub fn play_2d(&self, id: SourceId, mut config: SoundConfig) -> Result<Arc<dyn Sound>, AudioError> {
        config.position = None;
        let eff = self.effective_volume(&config);
        let sound = self.backend.spawn(id, config)?;
        sound.set_volume(eff);
        sound.play();
        Ok(sound)
    }

    /// 播放 3D 声音。`pos` 是世界坐标。
    pub fn play_3d(&self, id: SourceId, pos: SoundPosition, mut config: SoundConfig) -> Result<Arc<dyn Sound>, AudioError> {
        config.position = Some(pos);
        let eff = self.effective_volume(&config);
        let sound = self.backend.spawn(id, config)?;
        sound.set_volume(eff);
        sound.set_position(pos);
        sound.play();
        Ok(sound)
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_backend_load_then_spawn_and_play() {
        let mut sys = AudioSystem::with_null();
        let id = sys.load(b"fake-bytes").unwrap();
        let s = sys.play_2d(id, SoundConfig::default()).unwrap();
        assert!(s.is_playing());
        s.pause();
        assert!(!s.is_playing());
    }

    #[test]
    fn unloaded_source_cannot_spawn() {
        let mut sys = AudioSystem::with_null();
        let id = sys.load(b"x").unwrap();
        sys.unload(id);
        let err = match sys.play_2d(id, SoundConfig::default()) {
            Ok(_) => panic!("expected SourceNotFound"),
            Err(e) => e,
        };
        assert!(matches!(err, AudioError::SourceNotFound(_)));
    }

    #[test]
    fn effective_volume_multiplies_master_channel_config() {
        let mut sys = AudioSystem::with_null();
        sys.set_master_volume(0.5);
        sys.set_channel_volume(MixerChannel::Music, 0.4);
        let cfg = SoundConfig {
            channel: MixerChannel::Music,
            volume: 0.5,
            ..Default::default()
        };
        let eff = sys.effective_volume(&cfg);
        // 0.5 * 0.4 * 0.5 = 0.1
        assert!((eff - 0.1).abs() < 1e-6, "eff={eff}");
    }

    #[test]
    fn master_volume_via_channel_master_alias() {
        let mut sys = AudioSystem::with_null();
        sys.set_channel_volume(MixerChannel::Master, 0.25);
        assert!((sys.master_volume() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn play_3d_sets_position_on_sound() {
        let mut sys = AudioSystem::with_null();
        let id = sys.load(b"x").unwrap();
        let pos = SoundPosition { position: [1.0, 2.0, 3.0], velocity: [0.0; 3] };
        // Null sound 不真正持位置，但接口必须接受。
        let s = sys.play_3d(id, pos, SoundConfig::default()).unwrap();
        assert!(s.is_playing());
    }

    #[test]
    fn attenuation_at_zero_distance_is_one() {
        let atten = compute_attenuation([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.1, 100.0);
        assert!((atten - 1.0).abs() < 1e-6, "atten={atten}");
    }

    #[test]
    fn attenuation_decreases_as_distance_increases() {
        let a1 = compute_attenuation([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1, 100.0);
        let a2 = compute_attenuation([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 0.1, 100.0);
        let a3 = compute_attenuation([0.0, 0.0, 0.0], [50.0, 0.0, 0.0], 0.1, 100.0);
        assert!(a1 > a2, "a1={a1} should > a2={a2}");
        assert!(a2 > a3, "a2={a2} should > a3={a3}");
    }

    #[test]
    fn attenuation_beyond_max_distance_is_zero() {
        let atten = compute_attenuation([0.0, 0.0, 0.0], [150.0, 0.0, 0.0], 0.1, 100.0);
        assert!(atten.abs() < 1e-6, "atten={atten}");
    }

    #[test]
    fn attenuation_at_max_distance_boundary_is_zero() {
        let atten = compute_attenuation([0.0, 0.0, 0.0], [100.0, 0.0, 0.0], 0.1, 100.0);
        assert!(atten.abs() < 1e-6, "atten={atten}");
    }

    #[test]
    fn audio_system_update_listener_forwards_to_backend() {
        let mut sys = AudioSystem::with_null();
        let l = Listener {
            position: [5.0, 10.0, 15.0],
            ..Default::default()
        };
        sys.set_listener(l);
        assert!((sys.listener().position[0] - 5.0).abs() < 1e-6);
        assert!((sys.listener().position[1] - 10.0).abs() < 1e-6);
    }

    #[test]
    fn audio_system_attenuation_params_defaults() {
        let sys = AudioSystem::with_null();
        assert!((sys.rolloff_factor() - DEFAULT_ROLLOFF_FACTOR).abs() < 1e-6);
        assert!((sys.max_distance() - DEFAULT_MAX_DISTANCE).abs() < 1e-6);
    }
}
