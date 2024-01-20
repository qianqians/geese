use tokio::io::{self};
use tokio::net::TcpStream;

use crate::tcp_socket::{TcpReader, TcpWriter};

pub struct TcpConnect {
}

impl TcpConnect {
    pub async fn connect(host:String) -> Result<(TcpReader, TcpWriter), Box<dyn std::error::Error>> {
        let mut _socket = TcpStream::connect(host).await?;
        let (rd, wr) = io::split(_socket);
        Ok((
            TcpReader::new(rd),
            TcpWriter::new(wr)
        ))
    }
}
