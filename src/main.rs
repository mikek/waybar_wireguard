use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use wireguard_uapi::{DeviceInterface, WgSocket, get::Device};

#[derive(Serialize)]
struct State {
    state: &'static str,
    interface: String,
    #[serde(rename = "latest handshake", skip_serializing_if = "Option::is_none")]
    latest_handshake: Option<String>,
}

fn main() {
    let dev = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: waybar-wireguard <interface>");
        std::process::exit(2);
    });
    run(dev, "/sys/class/net")
}

fn run(dev: String, sysfs_dir: &str) {
    // Cheap presence check via sysfs -- no netlink, no caps required.
    let dev_path = format!("{sysfs_dir}/{dev}");
    if !Path::new(&dev_path).exists() {
        emit(&build_state(dev, None, Duration::ZERO));
        return;
    }

    let mut wg = match WgSocket::connect() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("connect: {e}");
            std::process::exit(1);
        }
    };

    match wg.get_device(DeviceInterface::from_name(dev.as_str())) {
        Ok(device) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            emit(&build_state(dev, Some(&device), now));
        }
        Err(e) => {
            // Interface exists but we couldn't read it -- perms, kernel weirdness.
            eprintln!("get_device({dev}): {e}");
            std::process::exit(1);
        }
    }
}

fn build_state(dev: String, device: Option<&Device>, now: Duration) -> State {
    match device {
        Some(d) => {
            let peers: Vec<PeerInfo> = d.peers.iter().map(PeerInfo::from).collect();
            State {
                state: "up",
                interface: d.ifname.clone(),
                latest_handshake: latest_handshake_label(&peers, now),
            }
        }
        None => State {
            state: "down",
            interface: dev,
            latest_handshake: None,
        },
    }
}

#[derive(Debug, Clone, Copy)]
struct PeerInfo {
    latest_handshake: Duration,
}

impl From<&wireguard_uapi::get::Peer> for PeerInfo {
    fn from(p: &wireguard_uapi::get::Peer) -> Self {
        Self {
            latest_handshake: p.last_handshake_time,
        }
    }
}

fn emit(state: &State) {
    match serde_jsonc::to_string(state) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("serialize: {e}");
            std::process::exit(1);
        }
    }
}

/// Pick the most-recent handshake across peers and humanize the age.
/// `None` if no peer has ever handshaken (or the clock is somehow behind).
fn latest_handshake_label(peers: &[PeerInfo], now: Duration) -> Option<String> {
    let latest = peers
        .iter()
        .map(|p| p.latest_handshake)
        .filter(|d| !d.is_zero())
        .max()?;
    let age = now.checked_sub(latest)?;
    Some(format!("{} ago", humanize(age)))
}

fn humanize(d: Duration) -> String {
    let mut secs = d.as_secs();
    let days = secs / 86_400;
    secs %= 86_400;
    let hours = secs / 3600;
    secs %= 3600;
    let minutes = secs / 60;
    secs %= 60;

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(plural(days, "day"));
    }
    if hours > 0 {
        parts.push(plural(hours, "hour"));
    }
    if minutes > 0 {
        parts.push(plural(minutes, "minute"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(plural(secs, "second"));
    }
    parts.join(", ")
}

fn plural(n: u64, unit: &str) -> String {
    if n == 1 {
        format!("{n} {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wireguard_uapi::get::{DeviceBuilder, Peer, PeerBuilder};

    fn mk_peer(handshake_secs: u64) -> Peer {
        PeerBuilder::default()
            .public_key([0u8; 32])
            .preshared_key([0u8; 32])
            .persistent_keepalive_interval(0u16)
            .last_handshake_time(Duration::from_secs(handshake_secs))
            .rx_bytes(0u64)
            .tx_bytes(0u64)
            .protocol_version(1u32)
            .build()
            .unwrap()
    }

    fn mk_device(ifname: &str, peers: Vec<Peer>) -> Device {
        DeviceBuilder::default()
            .ifindex(1u32)
            .ifname(ifname.to_owned())
            .listen_port(51820u16)
            .fwmark(0u32)
            .peers(peers)
            .build()
            .unwrap()
    }

    #[test]
    fn humanize_zero() {
        assert_eq!(humanize(Duration::ZERO), "0 seconds");
    }

    #[test]
    fn humanize_singular_unit() {
        assert_eq!(humanize(Duration::from_secs(1)), "1 second");
        assert_eq!(humanize(Duration::from_secs(60)), "1 minute");
        assert_eq!(humanize(Duration::from_secs(3600)), "1 hour");
        assert_eq!(humanize(Duration::from_secs(86_400)), "1 day");
    }

    #[test]
    fn humanize_plural_unit() {
        assert_eq!(humanize(Duration::from_secs(59)), "59 seconds");
        assert_eq!(humanize(Duration::from_secs(120)), "2 minutes");
    }

    #[test]
    fn humanize_skips_zero_leading_units() {
        // 1 hour + 0 minutes + 5 seconds -> minutes omitted
        assert_eq!(humanize(Duration::from_secs(3605)), "1 hour, 5 seconds");
    }

    #[test]
    fn humanize_minute_and_seconds() {
        assert_eq!(humanize(Duration::from_secs(80)), "1 minute, 20 seconds");
    }

    #[test]
    fn humanize_all_units() {
        let d = Duration::from_secs(2 * 86_400 + 3 * 3600 + 4 * 60 + 5);
        assert_eq!(humanize(d), "2 days, 3 hours, 4 minutes, 5 seconds");
    }

    #[test]
    fn plural_cases() {
        assert_eq!(plural(0, "item"), "0 items");
        assert_eq!(plural(1, "item"), "1 item");
        assert_eq!(plural(5, "item"), "5 items");
    }

    fn pi(secs: u64) -> PeerInfo {
        PeerInfo {
            latest_handshake: Duration::from_secs(secs),
        }
    }

    #[test]
    fn latest_handshake_no_peers() {
        assert_eq!(latest_handshake_label(&[], Duration::from_secs(100)), None);
    }

    #[test]
    fn latest_handshake_never_handshaken() {
        assert_eq!(
            latest_handshake_label(&[pi(0)], Duration::from_secs(100)),
            None
        );
    }

    #[test]
    fn latest_handshake_picks_max_across_peers() {
        assert_eq!(
            latest_handshake_label(&[pi(100), pi(200), pi(0)], Duration::from_secs(260)),
            Some("1 minute ago".to_owned())
        );
    }

    #[test]
    fn latest_handshake_returns_none_when_clock_behind() {
        assert_eq!(
            latest_handshake_label(&[pi(500)], Duration::from_secs(100)),
            None
        );
    }

    #[test]
    fn latest_handshake_renders_seconds() {
        assert_eq!(
            latest_handshake_label(&[pi(95)], Duration::from_secs(100)),
            Some("5 seconds ago".to_owned())
        );
    }

    #[test]
    fn peer_info_conversion_from_wireguard_peer() {
        let peer = mk_peer(42);
        let info: PeerInfo = (&peer).into();
        assert_eq!(info.latest_handshake, Duration::from_secs(42));
    }

    #[test]
    fn build_state_down_when_interface_absent() {
        let s = build_state("wg0".into(), None, Duration::ZERO);
        assert_eq!(s.state, "down");
        assert_eq!(s.interface, "wg0");
        assert!(s.latest_handshake.is_none());
    }

    #[test]
    fn build_state_up_uses_device_ifname_over_arg() {
        let dev = mk_device("wg7", vec![mk_peer(50)]);
        let s = build_state("wg0".into(), Some(&dev), Duration::from_secs(110));
        assert_eq!(s.state, "up");
        assert_eq!(s.interface, "wg7");
        assert_eq!(s.latest_handshake.as_deref(), Some("1 minute ago"));
    }
}
