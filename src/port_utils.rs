use log::debug;
use std::net::TcpListener;

/// Check if a process is listening on a specific TCP port using pure Rust
/// This approach tries to bind to the port - if it fails, something is already listening
pub fn is_port_listening(port: u16) -> bool {
    debug!(
        "Checking if port {} is listening by attempting to bind",
        port
    );

    // Try to bind to both IPv4 and IPv6 addresses
    let ipv4_addr = format!("127.0.0.1:{}", port);
    let ipv6_addr = format!("[::1]:{}", port);

    // Only an AddrInUse error means something is listening; other errors
    // (e.g. IPv6 loopback unavailable) must not be mistaken for a listener.
    let ipv4_in_use = is_addr_in_use(&ipv4_addr);
    let ipv6_in_use = is_addr_in_use(&ipv6_addr);

    let is_listening = ipv4_in_use || ipv6_in_use;

    debug!(
        "Port {} check: IPv4 in use: {}, IPv6 in use: {}, listening: {}",
        port, ipv4_in_use, ipv6_in_use, is_listening
    );

    is_listening
}

/// True only when the bind fails because the address is already taken.
/// Other errors (e.g. no IPv6 loopback) must not count as "in use".
fn is_addr_in_use(addr: &str) -> bool {
    matches!(TcpListener::bind(addr), Err(e) if e.kind() == std::io::ErrorKind::AddrInUse)
}

/// Check if a process is listening on any of the specified ports
pub fn is_any_port_listening(ports: &[u16]) -> bool {
    for &port in ports {
        if is_port_listening(port) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_listening_detects_bound_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        assert!(is_port_listening(port));
        drop(listener);
        assert!(!is_port_listening(port));
    }

    #[test]
    fn test_any_port_listening() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        let free = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert!(is_any_port_listening(&[free, port]));
        assert!(!is_any_port_listening(&[]));
    }
}
