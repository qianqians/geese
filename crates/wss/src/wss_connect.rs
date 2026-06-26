use url::Url;
use futures_util::stream::StreamExt;
use tungstenite::http::Request;
use tokio_tungstenite::connect_async;
use base64::Engine;

use crate::wss_socket::{WSSReader, WSSWriter};

pub struct WSSConnect {
}

impl WSSConnect {
    pub async fn connect(host:String) -> Result<(WSSReader, WSSWriter), Box<dyn std::error::Error>> {
        let url = Url::parse(host.as_str())?;
        let host_str = url.host_str().ok_or("Invalid host in WebSocket URL")?;
        
        // Generate random WebSocket key per RFC 6455
        let ws_key: [u8; 16] = rand::random();
        let ws_key_b64 = base64::engine::general_purpose::STANDARD.encode(&ws_key);
        
        let request = Request::builder()
            .method("GET")
            .uri(host)
            .header("Host", host_str)
            .header("Upgrade", "websocket")
            .header("Connection", "upgrade")
            .header("Sec-Websocket-Key", &ws_key_b64)
            .header("Sec-Websocket-Version", "13")
            .body(())?;

        let (_client, _) = connect_async(request).await?;

        let (write, read) = _client.split();
        Ok((
            WSSReader::new(read), 
            WSSWriter::new(write)
        ))
    }
}