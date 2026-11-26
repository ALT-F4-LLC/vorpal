//! TCP listener binding shared by `vorpal system services start` and the
//! config-binary port pick, so both work on hosts without IPv6.

use anyhow::{bail, Context as _, Result};
use std::{
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
};

/// Binds a TCP listener on `host:port`.
///
/// With an explicit `host`, binds exactly that address. Without one, binds
/// every interface: the IPv6 wildcard `[::]` first, which also accepts IPv4 on
/// dual-stack hosts, then `0.0.0.0` when the host has no IPv6 (the `[::]`
/// bind fails with `EAFNOSUPPORT` or `EADDRNOTAVAIL`). A port already in use
/// is reported as is, without trying IPv4.
///
/// Port `0` asks the OS for a free port; read it from `local_addr()`.
pub fn bind_tcp(host: Option<IpAddr>, port: u16) -> Result<TcpListener> {
    if let Some(host) = host {
        let addr = SocketAddr::new(host, port);
        return TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"));
    }

    let v6_addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, port));

    let v6_err = match TcpListener::bind(v6_addr) {
        Ok(listener) => return Ok(listener),
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            bail!("failed to bind {v6_addr}: {err}")
        }
        Err(err) => err,
    };

    let v4_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));

    match TcpListener::bind(v4_addr) {
        Ok(listener) => Ok(listener),
        Err(v4_err) => bail!("failed to bind {v6_addr} ({v6_err}) or {v4_addr} ({v4_err})"),
    }
}

/// Binds a tokio TCP listener with [`bind_tcp`]. Must run inside a tokio
/// runtime.
pub fn bind_tcp_tokio(host: Option<IpAddr>, port: u16) -> Result<tokio::net::TcpListener> {
    let listener = bind_tcp(host, port)?;

    listener
        .set_nonblocking(true)
        .context("failed to set listener non-blocking")?;

    tokio::net::TcpListener::from_std(listener).context("failed to register listener")
}

/// Returns a TCP port that is currently free on every interface, as the
/// config binary will bind it (`[::]`, or `0.0.0.0` without IPv6).
///
/// The OS assigns the port and the probe listener is closed before
/// returning, so another process could take it in between; the config binary
/// then fails to bind and reports the error.
pub fn free_port() -> Result<u16> {
    let listener = bind_tcp(None, 0).context("failed to find a free port")?;

    let port = listener
        .local_addr()
        .context("failed to read the free port")?
        .port();

    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Works whether or not the host has IPv6: falls back to `0.0.0.0`.
    #[test]
    fn bind_tcp_without_host_binds_every_interface() -> TestResult {
        let listener = bind_tcp(None, 0)?;
        let addr = listener.local_addr()?;

        assert!(addr.ip().is_unspecified(), "bound {addr}");
        assert_ne!(addr.port(), 0);

        Ok(())
    }

    #[test]
    fn bind_tcp_with_host_binds_exactly_that_address() -> TestResult {
        let listener = bind_tcp(Some(IpAddr::V4(Ipv4Addr::LOCALHOST)), 0)?;
        let addr = listener.local_addr()?;

        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

        Ok(())
    }

    #[test]
    fn bind_tcp_rejects_port_in_use() -> TestResult {
        let held = bind_tcp(None, 0)?;
        let port = held.local_addr()?.port();

        let Err(err) = bind_tcp(None, port) else {
            return Err(format!("second bind of port {port} succeeded").into());
        };

        assert!(err.to_string().contains("failed to bind"), "{err}");

        Ok(())
    }

    /// The returned port can be bound again by the config binary.
    #[test]
    fn free_port_returns_a_bindable_port() -> TestResult {
        let port = free_port()?;

        assert_ne!(port, 0);

        let _listener = bind_tcp(None, port)?;

        Ok(())
    }
}
