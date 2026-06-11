//! 物理引擎 RPC 客户端。
//!
//! 复用 `crates/tcp` 和 `crates/net` 模块，通过 TCP + msgpack
//! 与 Python 物理服务器通信。
//!
//! 协议帧格式（与 `crates/net::NetPack` 兼容）：
//! ```text
//! [4 字节 LE 长度] + [msgpack 编码的 body]
//! ```

use std::sync::Arc;

use net::{NetReader, NetReaderCallback, NetWriter};
use serde::{Deserialize, Serialize};
use tcp::tcp_connect::TcpConnect;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// 基础数据类型
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
// 请求消息（编辑器 → 物理服务器）
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct Request {
    method: String,
    argvs: Vec<serde_json::Value>,
}

// 响应结构体

#[derive(Debug, Deserialize)]
struct InitPhysicsRsp {
    #[serde(default)]
    scene_id: i64,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct LoadSceneRsp {
    #[serde(default)]
    body_count: i64,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct StepPhysicsRsp {
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct GetBodiesRsp {
    #[serde(default)]
    bodies: Vec<BodySnapshot>,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct ContactsRsp {
    #[serde(default)]
    contacts: Vec<ContactInfo>,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct CastRayRsp {
    hit: Option<RayHit>,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct ResetPhysicsRsp {
    #[serde(default)]
    error: String,
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
/// `physics_editor_server.py` 通信。
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

    /// 发送 RPC 请求并等待响应。
    async fn call(&self, method: &str, argvs: Vec<serde_json::Value>) -> Result<Vec<u8>, String> {
        let request = Request {
            method: method.to_string(),
            argvs,
        };
        // 使用 to_vec_named 产生 msgpack map（与 Python msgpack.dumps(dict) 兼容）
        let body = rmp_serde::to_vec_named(&request).map_err(|e| format!("encode request: {e}"))?;

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
        let rsp: InitPhysicsRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(rsp.scene_id)
    }

    /// 从 `.scene.json` 加载碰撞体。
    pub async fn load_scene(&self, manifest_path: &str) -> Result<i64, String> {
        let argvs = vec![serde_json::json!(manifest_path)];
        let resp = self.call("load_scene", argvs).await?;
        let rsp: LoadSceneRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(rsp.body_count)
    }

    /// 步进物理模拟。
    pub async fn step(&self, dt: f64) -> Result<(), String> {
        let argvs = vec![serde_json::json!(dt)];
        let resp = self.call("step_physics", argvs).await?;
        let rsp: StepPhysicsRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(())
    }

    /// 获取所有碰撞体快照。
    pub async fn get_bodies(&self) -> Result<Vec<BodySnapshot>, String> {
        let resp = self.call("get_bodies", vec![]).await?;
        let rsp: GetBodiesRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(rsp.bodies)
    }

    /// 获取碰撞事件。
    pub async fn get_contacts(&self) -> Result<Vec<ContactInfo>, String> {
        let resp = self.call("get_contacts", vec![]).await?;
        let rsp: ContactsRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(rsp.contacts)
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
        let rsp: CastRayRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(rsp.hit)
    }

    /// 重置物理世界。
    pub async fn reset(&self) -> Result<(), String> {
        let resp = self.call("reset_physics", vec![]).await?;
        let rsp: ResetPhysicsRsp =
            rmp_serde::from_slice(&resp).map_err(|e| format!("decode: {e}"))?;
        if !rsp.error.is_empty() {
            return Err(rsp.error);
        }
        Ok(())
    }
}

impl Drop for PhysicsClient {
    fn drop(&mut self) {
        self.reader_join.abort();
    }
}
