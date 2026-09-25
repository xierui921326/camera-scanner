use std::net::Ipv4Addr;

use crate::model::MAX_HOSTS;

/// Parse a single IPv4 (`192.168.1.10`) or CIDR (`192.168.1.0/24`) into the
/// list of host addresses it covers.
pub fn parse_targets(input: &str) -> Result<Vec<Ipv4Addr>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("目标不能为空".into());
    }

    let (addr_part, prefix) = match input.split_once('/') {
        Some((a, p)) => {
            let prefix: u8 = p
                .trim()
                .parse()
                .map_err(|_| format!("非法的 CIDR 前缀: {p}"))?;
            if prefix > 32 {
                return Err(format!("CIDR 前缀必须在 0-32 之间: {prefix}"));
            }
            (a.trim(), prefix)
        }
        None => (input, 32u8),
    };

    let base: Ipv4Addr = addr_part
        .parse()
        .map_err(|_| format!("非法的 IPv4 地址: {addr_part}"))?;

    let base_u32 = u32::from(base);
    let host_bits = 32 - prefix as u32;
    let count: u64 = 1u64 << host_bits;

    if count as usize > MAX_HOSTS {
        return Err(format!(
            "该网段包含 {count} 个地址，超过单次扫描上限 {MAX_HOSTS}，请缩小范围（如使用更大的前缀 /24）"
        ));
    }

    // For /31 and /32 every address is a host; otherwise drop network and
    // broadcast addresses to match conventional host enumeration.
    let mask = if prefix == 0 { 0 } else { u32::MAX << host_bits };
    let network = base_u32 & mask;

    let mut hosts = Vec::with_capacity(count as usize);
    if prefix >= 31 {
        for i in 0..count as u32 {
            hosts.push(Ipv4Addr::from(network + i));
        }
    } else {
        // Skip .0 (network) and the last (broadcast).
        for i in 1..(count as u32 - 1) {
            hosts.push(Ipv4Addr::from(network + i));
        }
    }
    Ok(hosts)
}

/// A target is allowed without explicit authorization when every host is in a
/// private (RFC1918) or loopback range. Otherwise the exact CIDR string must
/// have been added to the confirmed whitelist.
pub fn ensure_authorized(
    cidr: &str,
    hosts: &[Ipv4Addr],
    whitelist: &[String],
) -> Result<(), String> {
    let all_private = hosts
        .iter()
        .all(|ip| ip.is_private() || ip.is_loopback());
    if all_private {
        return Ok(());
    }
    if whitelist.iter().any(|w| w == cidr) {
        return Ok(());
    }
    Err(format!(
        "目标 {cidr} 包含非私有地址。仅允许扫描私有网段(RFC1918)或已确认授权并加入白名单的目标。"
    ))
}
