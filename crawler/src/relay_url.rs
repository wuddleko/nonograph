use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn canonical_public_relay(relay: &str) -> Option<String> {
    let relay = relay.trim();
    if !public_relay_url(relay) {
        return None;
    }
    let url = url::Url::parse(relay).ok()?;
    let host = match url.host()? {
        url::Host::Domain(domain) => domain.trim_end_matches('.').to_ascii_lowercase(),
        url::Host::Ipv4(address) => address.to_string(),
        url::Host::Ipv6(address) => format!("[{address}]"),
    };
    if host.is_empty() {
        return None;
    }
    Some(format!("wss://{host}"))
}

fn public_relay_url(relay: &str) -> bool {
    if !valid_relay_url(relay) {
        return false;
    }
    let Ok(url) = url::Url::parse(relay) else {
        return false;
    };
    if url.scheme() != "wss" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if url.port_or_known_default() != Some(443) {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(domain)) => {
            let host = domain.trim_end_matches('.').to_ascii_lowercase();
            if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
                return false;
            }
            if let Ok(ip) = host.parse::<IpAddr>() {
                return !blocked_relay_ip(ip);
            }
            true
        }
        Some(url::Host::Ipv4(v4)) => !blocked_relay_ip(IpAddr::V4(v4)),
        Some(url::Host::Ipv6(v6)) => !blocked_relay_ip(IpAddr::V6(v6)),
        None => false,
    }
}

fn valid_relay_url(relay: &str) -> bool {
    let Some(host) = relay.strip_prefix("wss://") else {
        return false;
    };
    !host.is_empty()
        && relay.is_ascii()
        && relay.len() <= 255
        && !relay.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn blocked_relay_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => blocked_relay_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return blocked_relay_ipv4(v4);
            }
            if let Some(v4) = v6.to_ipv4() {
                return blocked_relay_ipv4(v4);
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                || nat64_prefix(v6)
        }
    }
}

fn blocked_relay_ipv4(v4: Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        || v4.octets()[0] == 0
        || carrier_grade_nat(v4)
}

fn carrier_grade_nat(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn nat64_prefix(v6: Ipv6Addr) -> bool {
    let segments = v6.segments();
    segments[0] == 0x64
        && segments[1] == 0xff9b
        && segments[2] == 0
        && segments[3] == 0
        && segments[4] == 0
        && segments[5] == 0
}
