use std::sync::Arc;

use tokio::sync::Mutex;
use axum::{
    routing::get,
    http::StatusCode,
    extract::State,
    Router,
};
use tracing::info;

pub struct HealthHandle {
    _addr: String,
    status: bool
}

impl HealthHandle {
    pub fn new(_addr: String) -> Arc<Mutex<HealthHandle>> {
        Arc::new(Mutex::new(HealthHandle { _addr: _addr, status: true }))
    }
    
    async fn health_handle(State(state): State<Arc<Mutex<HealthHandle>>>) -> (StatusCode, &'static str) {
        let s = state.as_ref().lock().await;
        if s.status {
            info!("health state passing!");
            (StatusCode::OK, "passing!")
        }
        else {
            info!("health state busy!");
            (StatusCode::INTERNAL_SERVER_ERROR, "busy!")
        }
    }

    pub async fn start_health_service(host: String, handle: Arc<Mutex<HealthHandle>>) {
        let app = Router::new()
            .route("/health", get(HealthHandle::health_handle))
            .with_state(handle.clone());
        
        axum::Server::bind(&host.parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    }

    pub fn set_health_status(&mut self, _status: bool) {
        self.status = _status
    }

}
