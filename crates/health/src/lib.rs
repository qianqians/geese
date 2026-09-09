use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use axum::{
    routing::get,
    http::StatusCode,
    extract::State,
    Router,
};
use tracing::{debug, error};

/// 主循环若超过该时长未“心跳”，即判定为不健康。
/// 需小于 Consul 检查的 interval（10s），这里取 5s。
const HEALTHY_GRACE: Duration = Duration::from_secs(5);

pub struct HealthHandle {
    _addr: String,
    /// 最近一次心跳（主循环仍在正常运转）的时刻。
    last_heartbeat: Instant,
    /// 显式健康状态：true=健康，false=忙碌/下线。
    status: bool,
}

impl HealthHandle {
    pub fn new(_addr: String) -> Arc<Mutex<HealthHandle>> {
        Arc::new(Mutex::new(HealthHandle {
            _addr,
            last_heartbeat: Instant::now(),
            status: true,
        }))
    }

    async fn health_handle(
        State(state): State<Arc<Mutex<HealthHandle>>>,
    ) -> (StatusCode, &'static str) {
        let s = state.lock().await;
        let healthy = s.status && s.last_heartbeat.elapsed() < HEALTHY_GRACE;
        if healthy {
            debug!("health check passing");
            (StatusCode::OK, "ok")
        } else {
            debug!("health check failing");
            (StatusCode::SERVICE_UNAVAILABLE, "unhealthy")
        }
    }

    /// 主循环每轮调用一次，表示“我还活着”。
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// 显式设置健康状态（hub 由 Python 侧调用），同时刷新心跳时间。
    pub fn set_health_status(&mut self, status: bool) {
        self.last_heartbeat = Instant::now();
        self.status = status;
    }

    /// 绑定健康检查端口；成功时端口已处于 LISTEN 状态，
    /// 便于“先 bind、再注册 consul、最后 serve”，避免注册后端点未就绪的竞态。
    pub async fn bind(host: String) -> Result<tokio::net::TcpListener, Box<dyn std::error::Error>> {
        match tokio::net::TcpListener::bind(host).await {
            Ok(l) => Ok(l),
            Err(e) => {
                error!("health service bind failed: {}", e);
                Err(Box::new(e))
            }
        }
    }

    /// 用已绑定的 listener 提供 `/health` 服务。
    pub async fn serve(
        listener: tokio::net::TcpListener,
        handle: Arc<Mutex<HealthHandle>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/health", get(HealthHandle::health_handle))
            .with_state(handle);
        axum::serve(listener, app.into_make_service()).await?;
        Ok(())
    }
}
