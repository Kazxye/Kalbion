//! Link layer → IPv4/IPv6 → UDP. Returns the UDP payload with its endpoints, or why the
//! packet was skipped. Every length read from the packet is checked against the bytes that
//! are actually there.
use crate::pcap::{
    LINK_ETHERNET, LINK_IPV4, LINK_IPV6, LINK_LINUX_SLL, LINK_LINUX_SLL2, LINK_LOOP, LINK_NULL,
    LINK_RAW,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Endpoint {
    pub address: IpAddr,
    pub port: u16,
}

#[derive(Debug)]
pub struct Udp<'a> {
    pub source: Endpoint,
    pub destination: Endpoint,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Link type this reader does not understand.
    LinkType,
    /// Not IPv4/IPv6, or not UDP.
    NotUdp,
    /// IP-level fragment; Photon fragments its own messages, so these are not expected.
    IpFragment,
    /// The packet's headers say it is longer than the bytes recorded.
    Truncated,
    /// Headers that contradict themselves.
    Malformed,
}

const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_IPV6: u16 = 0x86DD;
const ETHERTYPE_VLAN: u16 = 0x8100;
const ETHERTYPE_QINQ: u16 = 0x88A8;
const UDP: u8 = 17;

pub fn udp(link_type: u32, frame: &[u8]) -> Result<Udp<'_>, Skip> {
    let (ethertype, ip) = match link_type {
        LINK_ETHERNET => ethernet(frame)?,
        LINK_LINUX_SLL => {
            let header = frame.get(..16).ok_or(Skip::Truncated)?;
            (u16::from_be_bytes([header[14], header[15]]), &frame[16..])
        }
        LINK_LINUX_SLL2 => {
            let header = frame.get(..20).ok_or(Skip::Truncated)?;
            (u16::from_be_bytes([header[0], header[1]]), &frame[20..])
        }
        LINK_NULL | LINK_LOOP => {
            let header = frame.get(..4).ok_or(Skip::Truncated)?;
            // The address family is in host byte order for NULL and network order for LOOP;
            // the recording host is unknown, so both orders are accepted.
            let family = [
                u32::from_le_bytes([header[0], header[1], header[2], header[3]]),
                u32::from_be_bytes([header[0], header[1], header[2], header[3]]),
            ];
            let ethertype = if family.contains(&2) {
                ETHERTYPE_IPV4
            } else if family.iter().any(|value| [24, 28, 30].contains(value)) {
                ETHERTYPE_IPV6
            } else {
                return Err(Skip::NotUdp);
            };
            (ethertype, &frame[4..])
        }
        LINK_RAW | LINK_IPV4 | LINK_IPV6 => match frame.first().map(|byte| byte >> 4) {
            Some(4) => (ETHERTYPE_IPV4, frame),
            Some(6) => (ETHERTYPE_IPV6, frame),
            Some(_) => return Err(Skip::NotUdp),
            None => return Err(Skip::Truncated),
        },
        _ => return Err(Skip::LinkType),
    };
    match ethertype {
        ETHERTYPE_IPV4 => ipv4(ip),
        ETHERTYPE_IPV6 => ipv6(ip),
        _ => Err(Skip::NotUdp),
    }
}

fn ethernet(frame: &[u8]) -> Result<(u16, &[u8]), Skip> {
    let mut offset = 12;
    // Up to two VLAN tags (802.1Q, 802.1ad).
    for _ in 0..3 {
        let bytes = frame.get(offset..offset + 2).ok_or(Skip::Truncated)?;
        let ethertype = u16::from_be_bytes([bytes[0], bytes[1]]);
        if ethertype == ETHERTYPE_VLAN || ethertype == ETHERTYPE_QINQ {
            offset += 4;
            continue;
        }
        return Ok((ethertype, &frame[offset + 2..]));
    }
    Err(Skip::Malformed)
}

fn ipv4(packet: &[u8]) -> Result<Udp<'_>, Skip> {
    let header = packet.get(..20).ok_or(Skip::Truncated)?;
    if header[0] >> 4 != 4 {
        return Err(Skip::Malformed);
    }
    let header_length = usize::from(header[0] & 0x0F) * 4;
    let total_length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    if header_length < 20 || total_length < header_length {
        return Err(Skip::Malformed);
    }
    if packet.len() < total_length {
        return Err(Skip::Truncated);
    }
    let flags_offset = u16::from_be_bytes([header[6], header[7]]);
    let more_fragments = flags_offset & 0x2000 != 0;
    if more_fragments || flags_offset & 0x1FFF != 0 {
        return Err(Skip::IpFragment);
    }
    if header[9] != UDP {
        return Err(Skip::NotUdp);
    }
    let source = IpAddr::V4(Ipv4Addr::new(
        header[12], header[13], header[14], header[15],
    ));
    let destination = IpAddr::V4(Ipv4Addr::new(
        header[16], header[17], header[18], header[19],
    ));
    datagram(&packet[header_length..total_length], source, destination)
}

fn ipv6(packet: &[u8]) -> Result<Udp<'_>, Skip> {
    let header = packet.get(..40).ok_or(Skip::Truncated)?;
    if header[0] >> 4 != 6 {
        return Err(Skip::Malformed);
    }
    let payload_length = usize::from(u16::from_be_bytes([header[4], header[5]]));
    let end = 40 + payload_length;
    if payload_length == 0 {
        // Jumbograms are not used by game traffic.
        return Err(Skip::Malformed);
    }
    if packet.len() < end {
        return Err(Skip::Truncated);
    }
    let address = |bytes: &[u8]| {
        let mut octets = [0u8; 16];
        octets.copy_from_slice(bytes);
        IpAddr::V6(Ipv6Addr::from(octets))
    };
    let source = address(&header[8..24]);
    let destination = address(&header[24..40]);
    let mut next = header[6];
    let mut offset = 40;
    // Hop-by-hop, routing and destination options may precede UDP; a fragment header means
    // an IP-level fragment.
    for _ in 0..8 {
        match next {
            UDP => return datagram(&packet[offset..end], source, destination),
            0 | 43 | 60 => {
                let extension = packet.get(offset..offset + 2).ok_or(Skip::Truncated)?;
                next = extension[0];
                offset += (usize::from(extension[1]) + 1) * 8;
                if offset > end {
                    return Err(Skip::Malformed);
                }
            }
            44 => return Err(Skip::IpFragment),
            _ => return Err(Skip::NotUdp),
        }
    }
    Err(Skip::Malformed)
}

fn datagram(segment: &[u8], source: IpAddr, destination: IpAddr) -> Result<Udp<'_>, Skip> {
    let header = segment.get(..8).ok_or(Skip::Truncated)?;
    let length = usize::from(u16::from_be_bytes([header[4], header[5]]));
    if length < 8 {
        return Err(Skip::Malformed);
    }
    if segment.len() < length {
        return Err(Skip::Truncated);
    }
    Ok(Udp {
        source: Endpoint {
            address: source,
            port: u16::from_be_bytes([header[0], header[1]]),
        },
        destination: Endpoint {
            address: destination,
            port: u16::from_be_bytes([header[2], header[3]]),
        },
        payload: &segment[8..length],
    })
}
