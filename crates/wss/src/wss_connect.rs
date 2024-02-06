use url::Url;
use futures_util::stream::StreamExt;
use tokio_tungstenite::connect_async;

use crate::wss_socket::{WSSReader, WSSWriter};

pub struct WSSConnect {
}

impl WSSConnect {
    pub async fn connect(host:String) -> Result<(WSSReader, WSSWriter), Box<dyn std::error::Error>> {
        let url = Url::parse(host.as_str()).unwrap();
        let (_client, _) = connect_async(url).await.expect("failed to connect!");

        let (write, read) = _client.split();
        Ok((
            WSSReader::new(read), 
            WSSWriter::new(write)
        ))
    }
}