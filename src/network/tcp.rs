use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

use crate::network::error::NetworkError;

pub async fn bind_listener<A>(addr: A) -> Result<TcpListener, NetworkError>
where
    A: ToSocketAddrs,
{
    Ok(TcpListener::bind(addr).await?)
}

pub async fn connect_peer(addr: SocketAddr) -> Result<TcpStream, NetworkError> {
    Ok(TcpStream::connect(addr).await?)
}

pub async fn accept_peer(listener: &TcpListener) -> Result<(TcpStream, SocketAddr), NetworkError> {
    Ok(listener.accept().await?)
}
