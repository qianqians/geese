use tokio::io::{self};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::tcp_socket::{TcpReader, TcpWriter};

pub struct TcpConnect {
}

impl TcpConnect {
    pub async fn connect(host:String) -> Result<(TcpReader, TcpWriter), Box<dyn std::error::Error>> {
        let mut _socket = timeout(Duration::from_secs(5), TcpStream::connect(host)).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "Connection timed out after 5s"))??;
        let (rd, wr) = io::split(_socket);
        Ok((
            TcpReader::new(rd),
            TcpWriter::new(wr)
        ))
    }
}
