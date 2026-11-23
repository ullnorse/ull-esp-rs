//! A DNS server that answers every name with the board's address.
//!
//! Phones test a known page after joining a network. When that test lands on
//! the board instead, they open the setup page by themselves.

use core::net::Ipv4Addr;

pub const PORT: u16 = 53;
pub const PACKET_MAX: usize = 512;

const TYPE_A: u16 = 1;
const TYPE_ANY: u16 = 255;
const CLASS_IN: u16 = 1;
const TTL_SECONDS: u32 = 10;

/// Builds the answer to `query` in `out`; `None` for anything that is not a
/// single standard query.
pub fn answer(query: &[u8], ip: Ipv4Addr, out: &mut [u8; PACKET_MAX]) -> Option<usize> {
    let header = query.get(..12)?;
    let flags = u16::from_be_bytes([header[2], header[3]]);
    let is_query = flags & 0x8000 == 0;
    let opcode = (flags >> 11) & 0xf;
    let questions = u16::from_be_bytes([header[4], header[5]]);
    if !is_query || opcode != 0 || questions != 1 {
        return None;
    }

    // The question: labels up to a zero byte, then type and class.
    let mut end = 12;
    loop {
        let len = usize::from(*query.get(end)?);
        if len == 0 {
            end += 1;
            break;
        }
        if len > 63 {
            return None; // compression is not used in questions
        }
        end += 1 + len;
    }
    let fixed = query.get(end..end + 4)?;
    let qtype = u16::from_be_bytes([fixed[0], fixed[1]]);
    let qclass = u16::from_be_bytes([fixed[2], fixed[3]]);
    end += 4;
    let question = &query[12..end];
    if end + 16 > PACKET_MAX {
        return None;
    }

    let has_answer = qclass == CLASS_IN && (qtype == TYPE_A || qtype == TYPE_ANY);
    out[0..2].copy_from_slice(&header[0..2]); // id
    let recursion_desired = flags & 0x0100;
    out[2..4].copy_from_slice(&(0x8480 | recursion_desired).to_be_bytes()); // QR AA RA
    out[4..6].copy_from_slice(&1u16.to_be_bytes());
    out[6..8].copy_from_slice(&u16::from(has_answer).to_be_bytes());
    out[8..12].fill(0);
    out[12..end].copy_from_slice(question);
    let mut len = end;
    if has_answer {
        out[len..len + 2].copy_from_slice(&0xc00cu16.to_be_bytes()); // name: the question's
        out[len + 2..len + 4].copy_from_slice(&TYPE_A.to_be_bytes());
        out[len + 4..len + 6].copy_from_slice(&CLASS_IN.to_be_bytes());
        out[len + 6..len + 10].copy_from_slice(&TTL_SECONDS.to_be_bytes());
        out[len + 10..len + 12].copy_from_slice(&4u16.to_be_bytes());
        out[len + 12..len + 16].copy_from_slice(&ip.octets());
        len += 16;
    }
    Some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(name: &str, qtype: u16) -> Vec<u8> {
        let mut packet = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
        packet.push(0);
        packet.extend(qtype.to_be_bytes());
        packet.extend(CLASS_IN.to_be_bytes());
        packet
    }

    const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);

    #[test]
    fn every_name_points_at_the_board() {
        let q = query("captive.apple.com", TYPE_A);
        let mut out = [0u8; PACKET_MAX];
        let len = answer(&q, IP, &mut out).unwrap();
        assert_eq!(&out[0..2], &[0x12, 0x34]);
        assert_eq!(u16::from_be_bytes([out[6], out[7]]), 1);
        assert_eq!(&out[12..q.len()], &q[12..]);
        assert_eq!(&out[len - 4..len], &IP.octets());
    }

    #[test]
    fn ipv6_lookups_get_no_address() {
        let q = query("example.com", 28);
        let mut out = [0u8; PACKET_MAX];
        let len = answer(&q, IP, &mut out).unwrap();
        assert_eq!(len, q.len());
        assert_eq!(u16::from_be_bytes([out[6], out[7]]), 0);
    }

    #[test]
    fn junk_is_ignored() {
        let mut out = [0u8; PACKET_MAX];
        assert_eq!(answer(&[1, 2, 3], IP, &mut out), None);
        let mut response = query("a.b", TYPE_A);
        response[2] |= 0x80;
        assert_eq!(answer(&response, IP, &mut out), None);
        let truncated = &query("example.com", TYPE_A)[..15];
        assert_eq!(answer(truncated, IP, &mut out), None);
    }
}
