//! 物理引擎 RPC 客户端（RPC 数组协议版）。
//!
//! 复用 `crates/tcp` 和 `crates/net` 模块，通过 TCP + msgpack
//! 与 Python 物理服务器通信。
//!
//! 协议帧格式（与 `crates/net::NetPack` 兼容）：
//! ```text
//! [4 字节 LE 长度] + [msgpack 编码的 body]
//! ```
//!
//! 请求 body: `[method_str, args_array]` — msgpack 数组，与 RPC stubs 对齐
//! 响应 body: `[result1, result2, ...]` — 成功数组
//! 错误响应: `["__err__", err_str]`

use std::sync::Arc;

use net::{NetReader, NetReaderCallback, NetWriter};
use serde::{Deserialize, Serialize};
use tcp::tcp_connect::TcpConnect;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// 基础数据类型（与 physics_common.juggle struct 对齐）
// ---------------------------------------------------------------------------

/// 三维向量。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// 四元数。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

/// 碰撞体快照（调试渲染用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodySnapshot {
    pub id: String,
    pub position: Vec3,
    /// 四元数旋转（x, y, z, w）
    pub rotation: Quat,
}

/// 射线检测命中结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayHit {
    pub body_id: String,
    pub point: Vec3,
    pub normal: Vec3,
}

/// 碰撞接触事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    pub body1_id: String,
    pub body2_id: String,
    pub point: Vec3,
    pub normal: Vec3,
}

// ---------------------------------------------------------------------------
// RPC 响应解析
// ---------------------------------------------------------------------------

const ERR_TAG: &str = "__err__";

/// 从 msgpack 数组响应中解析结果。
/// - 成功: 返回 `Ok(array)` 供方法按索引取值
/// - 错误: `["__err__", err_str]` → 返回 `Err(err_str)`
fn parse_response(data: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    let arr: Vec<serde_json::Value> =
        rmp_serde::from_slice(data).map_err(|e| format!("decode response: {e}"))?;
    if arr.len() >= 2 {
        if let Some(tag) = arr[0].as_str() {
            if tag == ERR_TAG {
                let err_msg = arr[1].as_str().unwrap_or("unknown error").to_string();
                return Err(err_msg);
            }
        }
    }
    Ok(arr)
}

/// 从响应数组中安全取值，空数组时返回描述性错误而非 panic。
fn get_field<'a>(arr: &'a [serde_json::Value], idx: usize, name: &str) -> Result<&'a serde_json::Value, String> {
    arr.get(idx)
        .ok_or_else(|| format!("{name}: response array missing element [{idx}]"))
}

// ---------------------------------------------------------------------------
// 内部响应回调
// ---------------------------------------------------------------------------

struct PhysicsReaderCallback {
    rx: Arc<Mutex<Option<Vec<u8>>>>,
}

impl PhysicsReaderCallback {
    fn new() -> Self {
        Self {
            rx: Arc::new(Mutex::new(None)),
        }
    }

    async fn wait(&self) -> Result<Vec<u8>, String> {
        let start = tokio::time::Instant::now();
        let timeout = tokio::time::Duration::from_secs(30);
        loop {
            {
                let mut guard = self.rx.lock().await;
                if let Some(data) = guard.take() {
                    return Ok(data);
                }
            }
            if start.elapsed() > timeout {
                return Err("timeout after 30s".to_string());
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    }
}

#[async_trait::async_trait]
impl NetReaderCallback for PhysicsReaderCallback {
    async fn cb(&mut self, data: Vec<u8>) {
        let mut guard = self.rx.lock().await;
        *guard = Some(data);
    }
}

// ---------------------------------------------------------------------------
// PhysicsClient
// ---------------------------------------------------------------------------

/// 物理引擎 TCP 客户端。
///
/// 封装 `tcp::TcpConnect` + `net::NetWriter/NetReader`，与 Python
/// `physics_editor_server.py` 通信（RPC 数组协议）。
pub struct PhysicsClient {
    writer: Mutex<Box<dyn NetWriter + Send>>,
    callback: Arc<Mutex<PhysicsReaderCallback>>,
    reader_join: tokio::task::JoinHandle<()>,
}

impl PhysicsClient {
    /// 连接到物理服务器。
    pub async fn connect(host: &str) -> Result<Self, String> {
        let (reader, writer) = TcpConnect::connect(host.to_string())
            .await
            .map_err(|e| format!("TcpConnect failed: {e}"))?;

        let callback = PhysicsReaderCallback::new();
        let cb: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>> =
            Arc::new(Mutex::new(Box::new(PhysicsReaderCallback {
                rx: callback.rx.clone(),
            })));

        let reader_join = reader.start(cb);

        Ok(Self {
            writer: Mutex::new(Box::new(writer)),
            callback: Arc::new(Mutex::new(callback)),
            reader_join,
        })
    }

    /// 发送 RPC 请求（数组格式）并等待响应。
    ///
    /// 请求 body: `[method_str, args_array]`
    async fn call(&self, method: &str, argvs: Vec<serde_json::Value>) -> Result<Vec<u8>, String> {
        // 编码为 msgpack 数组: [method, args]
        let request: (String, Vec<serde_json::Value>) =
            (method.to_string(), argvs);
        let body =
            rmp_serde::to_vec(&request).map_err(|e| format!("encode request: {e}"))?;

        {
            let mut writer = self.writer.lock().await;
            writer.send(&body).await;
        }

        let resp = self.callback.lock().await.wait().await?;
        Ok(resp)
    }

    // ---- 便捷方法 ----

    /// 初始化物理世界。
    pub async fn init_physics(&self, gravity: [f32; 3]) -> Result<i64, String> {
        let argvs = vec![
            serde_json::json!(gravity[0]),
            serde_json::json!(gravity[1]),
            serde_json::json!(gravity[2]),
        ];
        let resp = self.call("init_physics", argvs).await?;
        let arr = parse_response(&resp)?;
        get_field(&arr, 0, "init_physics")?
            .as_i64()
            .ok_or_else(|| "init_physics: missing scene_id".to_string())
    }

    /// 从 `.scene.json` 加载碰撞体。
    pub async fn load_scene(&self, manifest_path: &str) -> Result<i64, String> {
        let argvs = vec![serde_json::json!(manifest_path)];
        let resp = self.call("load_scene", argvs).await?;
        let arr = parse_response(&resp)?;
        get_field(&arr, 0, "load_scene")?
            .as_i64()
            .ok_or_else(|| "load_scene: missing body_count".to_string())
    }

    /// 步进物理模拟。
    pub async fn step(&self, dt: f64) -> Result<(), String> {
        let argvs = vec![serde_json::json!(dt)];
        let resp = self.call("step_physics", argvs).await?;
        let _arr = parse_response(&resp)?;
        Ok(())
    }

    /// 获取所有碰撞体快照。
    pub async fn get_bodies(&self) -> Result<Vec<BodySnapshot>, String> {
        let resp = self.call("get_bodies", vec![]).await?;
        let arr = parse_response(&resp)?;
        let bodies: Vec<BodySnapshot> = serde_json::from_value(
            get_field(&arr, 0, "get_bodies")?.clone(),
        )
        .map_err(|e| format!("decode bodies: {e}"))?;
        Ok(bodies)
    }

    /// 获取碰撞事件。
    pub async fn get_contacts(&self) -> Result<Vec<ContactInfo>, String> {
        let resp = self.call("get_contacts", vec![]).await?;
        let arr = parse_response(&resp)?;
        let contacts: Vec<ContactInfo> = serde_json::from_value(
            get_field(&arr, 0, "get_contacts")?.clone(),
        )
        .map_err(|e| format!("decode contacts: {e}"))?;
        Ok(contacts)
    }

    /// 射线检测。
    pub async fn cast_ray(
        &self,
        origin: (f64, f64, f64),
        direction: (f64, f64, f64),
        max_toi: f64,
    ) -> Result<Option<RayHit>, String> {
        let argvs = vec![
            serde_json::json!(origin.0),
            serde_json::json!(origin.1),
            serde_json::json!(origin.2),
            serde_json::json!(direction.0),
            serde_json::json!(direction.1),
            serde_json::json!(direction.2),
            serde_json::json!(max_toi),
        ];
        let resp = self.call("cast_ray", argvs).await?;
        let arr = parse_response(&resp)?;
        // rsp: [ray_hit_dict|null, has_hit:bool]
        let has_hit = arr.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
        let hit_val = get_field(&arr, 0, "cast_ray")?;
        if !has_hit || hit_val.is_null() {
            return Ok(None);
        }
        let hit: RayHit = serde_json::from_value(hit_val.clone())
            .map_err(|e| format!("decode ray_hit: {e}"))?;
        Ok(Some(hit))
    }

    /// 重置物理世界。
    pub async fn reset(&self) -> Result<(), String> {
        let resp = self.call("reset_physics", vec![]).await?;
        let _arr = parse_response(&resp)?;
        Ok(())
    }
}

impl Drop for PhysicsClient {
    fn drop(&mut self) {
        self.reader_join.abort();
    }
}
