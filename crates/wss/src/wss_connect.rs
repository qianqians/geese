use std::sync::Arc;
use url::Url;

use tokio::sync::Mutex;
use tungstenite::connect;

use crate::wss_socket::{WSSReader, WSSWriter};

pub struct WSSConnect {
}

impl WSSConnect {
    pub async fn connect(host:String) -> Result<(WSSReader, WSSWriter), Box<dyn std::error::Error>> {
        let (_client, _) = 
            connect(Url::parse(host.as_str()).unwrap()).unwrap();

        let _s = Arc::new(Mutex::new(_client));
        let _s_clone = _s.clone();
        Ok((
            WSSReader::new(_s), 
            WSSWriter::new(_s_clone)
        ))
    }
}