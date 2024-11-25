use url::Url;
use futures_util::stream::StreamExt;
use tungstenite::http::Request;
use tokio_tungstenite::connect_async;

use crate::wss_socket::{WSSReader, WSSWriter};

pub struct WSSConnect {
}

impl WSSConnect {
    pub async fn connect(host:String) -> Result<(WSSReader, WSSWriter), Box<dyn std::error::Error>> {
        let url = Url::parse(host.as_str()).unwrap();
        let host_str = url.host_str().expect("Invalid host in WebSocket URL");
        let request = Request::builder()
            .method("GET")
            .uri(host)
            .header("Host", host_str)
            .header("Upgrade", "websocket")
            .header("Connection", "upgrade")
            .header("Sec-Websocket-Key", "key123")
            .header("Sec-Websocket-Version", "13")
            .body(())
            .unwrap();

        let (_client, _) = connect_async(request).await.unwrap();

        let (write, read) = _client.split();
        Ok((
            WSSReader::new(read), 
            WSSWriter::new(write)
        ))
    }
}