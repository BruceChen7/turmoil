use std::{io::Result, net::SocketAddr, sync::Arc};

use tokio::sync::Notify;

use crate::{
    net::{SocketPair, TcpStream},
    world::World,
    ToSocketAddrs, TRACING_TARGET,
};

/// A simulated TCP socket server, listening for connections.
///
/// All methods must be called from a host within a Turmoil simulation.
pub struct TcpListener {
    local_addr: SocketAddr,
    notify: Arc<Notify>,
}

impl TcpListener {
    pub(crate) fn new(local_addr: SocketAddr, notify: Arc<Notify>) -> Self {
        Self { local_addr, notify }
    }

    /// Creates a new TcpListener, which will be bound to the specified address.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Only 0.0.0.0 is currently supported.
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<TcpListener> {
        World::current(|world| {
            let mut addr = addr.to_socket_addr(&world.dns);
            let host = world.current_host_mut();

            if !addr.ip().is_unspecified() {
                panic!("{addr} is not supported");
            }

            tracing::info!("binding to {addr}");

            // Unspecified -> host's IP
            addr.set_ip(host.addr);

            // 绑定主机的ip
            host.tcp.bind(addr)
        })
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`TcpStream`] and the remote peer’s
    /// address will be returned.
    pub async fn accept(&self) -> Result<(TcpStream, SocketAddr)> {
        loop {
            //
            // 模拟主机接收到tcp连接
            let maybe_accept = World::current(|world| {
                let host = world.current_host_mut();
                // 从队列中获取一个连接
                let (syn, origin) = host.tcp.accept(self.local_addr)?;

                tracing::trace!(target: TRACING_TARGET, dst = ?origin, src = ?self.local_addr, protocol = %"TCP SYN", "Recv");

                // Send SYN-ACK -> origin. If Ok we proceed (acts as the ACK),
                // else we return early to avoid host mutations.
                let ack = syn.ack.send(());
                tracing::trace!(target: TRACING_TARGET, src = ?self.local_addr, dst = ?origin, protocol = %"TCP SYN-ACK", "Send");
                tracing::info!(target: TRACING_TARGET, src = ?self.local_addr, dst = ?origin, protocol = %"TCP SYN-ACK", "Sent");

                // 出现错误
                if ack.is_err() {
                    return None;
                }

                let pair = SocketPair::new(self.local_addr, origin);
                let rx = host.tcp.new_stream(pair);

                Some((TcpStream::new(pair, rx), origin))
            });

            if let Some(accepted) = maybe_accept {
                return Ok(accepted);
            }

            // 没有收到tcp连接，等待一段时间
            self.notify.notified().await;
        }
    }

    /// Returns the local address that this listener is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.local_addr)
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        World::current_if_set(|world| world.current_host_mut().tcp.unbind(self.local_addr));
    }
}
