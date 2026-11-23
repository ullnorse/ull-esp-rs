//! A DHCP server for the few phones that join the setup network.
//!
//! The board is the router and DNS server at `.1`; clients get `.2` to `.9`
//! of the same /24. Each MAC keeps its address until the table is full, then
//! the oldest entry is reused.

use core::fmt::Write as _;
use core::net::Ipv4Addr;

use crate::settings::Text;

pub const SERVER_PORT: u16 = 67;
pub const CLIENT_PORT: u16 = 68;
/// Large enough for any request a phone sends and for our replies.
pub const PACKET_MAX: usize = 576;

const LEASES: usize = 8;
const LEASE_SECONDS: u32 = 3600;
const COOKIE: [u8; 4] = [99, 130, 83, 99];
const OPTIONS_START: usize = 240;

const DISCOVER: u8 = 1;
const OFFER: u8 = 2;
const REQUEST: u8 = 3;
const ACK: u8 = 5;
const NAK: u8 = 6;

const OPT_SUBNET: u8 = 1;
const OPT_ROUTER: u8 = 3;
const OPT_DNS: u8 = 6;
const OPT_REQUESTED_IP: u8 = 50;
const OPT_LEASE_TIME: u8 = 51;
const OPT_MESSAGE_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
/// RFC 8910: tells the phone where the sign-in page is.
const OPT_CAPTIVE_PORTAL: u8 = 114;
const OPT_END: u8 = 255;

pub struct DhcpServer {
    ip: Ipv4Addr,
    leases: [Option<[u8; 6]>; LEASES],
    next: usize,
}

impl DhcpServer {
    /// `ip` is the board's own address; it must end in `.1`.
    pub const fn new(ip: Ipv4Addr) -> Self {
        Self {
            ip,
            leases: [None; LEASES],
            next: 0,
        }
    }

    /// Answers one request. Returns the reply length in `out`, to be sent to
    /// `255.255.255.255:68`, or `None` when there is nothing to answer.
    pub fn handle(&mut self, request: &[u8], out: &mut [u8; PACKET_MAX]) -> Option<usize> {
        if request.len() < OPTIONS_START || request[0] != 1 || request[2] != 6 {
            return None;
        }
        if request[236..240] != COOKIE {
            return None;
        }
        let mac: [u8; 6] = request[28..34].try_into().ok()?;
        let options = &request[OPTIONS_START..];
        let kind = option(options, OPT_MESSAGE_TYPE).and_then(|value| value.first().copied())?;

        let reply = match kind {
            DISCOVER => (OFFER, self.lease(mac)),
            REQUEST => {
                if let Some(server) = option(options, OPT_SERVER_ID)
                    && server != self.ip.octets()
                {
                    return None; // the phone picked another server
                }
                let leased = self.lease(mac);
                let wanted = option(options, OPT_REQUESTED_IP)
                    .and_then(|value| <[u8; 4]>::try_from(value).ok())
                    .map(Ipv4Addr::from)
                    .or_else(|| {
                        let ciaddr =
                            Ipv4Addr::new(request[12], request[13], request[14], request[15]);
                        (!ciaddr.is_unspecified()).then_some(ciaddr)
                    });
                match wanted {
                    Some(wanted) if wanted != leased => (NAK, Ipv4Addr::UNSPECIFIED),
                    _ => (ACK, leased),
                }
            }
            _ => return None,
        };
        Some(self.reply(request, reply.0, reply.1, out))
    }

    fn lease(&mut self, mac: [u8; 6]) -> Ipv4Addr {
        let index = match self.leases.iter().position(|lease| *lease == Some(mac)) {
            Some(index) => index,
            None => {
                let index = self.next;
                self.leases[index] = Some(mac);
                self.next = (index + 1) % LEASES;
                index
            }
        };
        let [a, b, c, _] = self.ip.octets();
        Ipv4Addr::new(a, b, c, 2 + index as u8)
    }

    fn reply(
        &self,
        request: &[u8],
        kind: u8,
        yiaddr: Ipv4Addr,
        out: &mut [u8; PACKET_MAX],
    ) -> usize {
        out.fill(0);
        out[0] = 2; // BOOTREPLY
        out[1] = 1; // Ethernet
        out[2] = 6;
        out[4..8].copy_from_slice(&request[4..8]); // xid
        out[10..12].copy_from_slice(&request[10..12]); // flags
        out[16..20].copy_from_slice(&yiaddr.octets());
        if kind != NAK {
            out[20..24].copy_from_slice(&self.ip.octets()); // siaddr
        }
        out[28..44].copy_from_slice(&request[28..44]); // chaddr
        out[236..240].copy_from_slice(&COOKIE);

        let ip = self.ip.octets();
        let mut writer = OptionWriter {
            out,
            len: OPTIONS_START,
        };
        writer.put(OPT_MESSAGE_TYPE, &[kind]);
        writer.put(OPT_SERVER_ID, &ip);
        if kind != NAK {
            writer.put(OPT_LEASE_TIME, &LEASE_SECONDS.to_be_bytes());
            writer.put(OPT_SUBNET, &[255, 255, 255, 0]);
            writer.put(OPT_ROUTER, &ip);
            writer.put(OPT_DNS, &ip);
            let mut url = Text::<23>::empty();
            let _ = write!(url, "http://{}/", self.ip);
            writer.put(OPT_CAPTIVE_PORTAL, url.as_str().as_bytes());
        }
        writer.out[writer.len] = OPT_END;
        // Some clients drop replies shorter than a BOOTP packet (300 bytes).
        (writer.len + 1).max(300)
    }
}

struct OptionWriter<'a> {
    out: &'a mut [u8; PACKET_MAX],
    len: usize,
}

impl OptionWriter<'_> {
    fn put(&mut self, code: u8, value: &[u8]) {
        self.out[self.len] = code;
        self.out[self.len + 1] = value.len() as u8;
        self.out[self.len + 2..self.len + 2 + value.len()].copy_from_slice(value);
        self.len += 2 + value.len();
    }
}

/// The value of option `code`, skipping pads.
fn option(mut options: &[u8], code: u8) -> Option<&[u8]> {
    loop {
        match options {
            [0, rest @ ..] => options = rest,
            [OPT_END, ..] | [] => return None,
            [found, len, rest @ ..] => {
                let value = rest.get(..usize::from(*len))?;
                if *found == code {
                    return Some(value);
                }
                options = &rest[usize::from(*len)..];
            }
            [_] => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);
    const MAC: [u8; 6] = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0x01];

    fn request(mac: [u8; 6], kind: u8, extra: &[(u8, &[u8])]) -> Vec<u8> {
        let mut packet = vec![0u8; OPTIONS_START];
        packet[0] = 1;
        packet[1] = 1;
        packet[2] = 6;
        packet[4..8].copy_from_slice(&[1, 2, 3, 4]);
        packet[28..34].copy_from_slice(&mac);
        packet[236..240].copy_from_slice(&COOKIE);
        packet.extend([OPT_MESSAGE_TYPE, 1, kind]);
        for (code, value) in extra {
            packet.extend([*code, value.len() as u8]);
            packet.extend_from_slice(value);
        }
        packet.push(OPT_END);
        packet
    }

    fn answer(server: &mut DhcpServer, packet: &[u8]) -> Option<(u8, Ipv4Addr, Vec<u8>)> {
        let mut out = [0u8; PACKET_MAX];
        let len = server.handle(packet, &mut out)?;
        let reply = out[..len].to_vec();
        let kind = option(&reply[OPTIONS_START..], OPT_MESSAGE_TYPE).unwrap()[0];
        let yiaddr = Ipv4Addr::new(reply[16], reply[17], reply[18], reply[19]);
        Some((kind, yiaddr, reply))
    }

    #[test]
    fn discover_then_request_gets_the_same_address() {
        let mut server = DhcpServer::new(SERVER);
        let (kind, offered, reply) = answer(&mut server, &request(MAC, DISCOVER, &[])).unwrap();
        assert_eq!((kind, offered), (OFFER, Ipv4Addr::new(192, 168, 4, 2)));
        assert_eq!(&reply[4..8], &[1, 2, 3, 4]);
        let options = &reply[OPTIONS_START..];
        assert_eq!(option(options, OPT_DNS), Some(&SERVER.octets()[..]));
        assert_eq!(
            option(options, OPT_CAPTIVE_PORTAL),
            Some(&b"http://192.168.4.1/"[..])
        );

        let (kind, acked, _) = answer(
            &mut server,
            &request(
                MAC,
                REQUEST,
                &[
                    (OPT_REQUESTED_IP, &offered.octets()),
                    (OPT_SERVER_ID, &SERVER.octets()),
                ],
            ),
        )
        .unwrap();
        assert_eq!((kind, acked), (ACK, offered));
    }

    #[test]
    fn second_phone_gets_the_next_address() {
        let mut server = DhcpServer::new(SERVER);
        answer(&mut server, &request(MAC, DISCOVER, &[])).unwrap();
        let mut other = MAC;
        other[5] = 2;
        let (_, ip, _) = answer(&mut server, &request(other, DISCOVER, &[])).unwrap();
        assert_eq!(ip, Ipv4Addr::new(192, 168, 4, 3));
    }

    #[test]
    fn address_from_another_network_is_refused() {
        let mut server = DhcpServer::new(SERVER);
        let (kind, _, _) = answer(
            &mut server,
            &request(MAC, REQUEST, &[(OPT_REQUESTED_IP, &[10, 0, 0, 7])]),
        )
        .unwrap();
        assert_eq!(kind, NAK);
    }

    #[test]
    fn request_for_another_server_is_ignored() {
        let mut server = DhcpServer::new(SERVER);
        assert!(
            answer(
                &mut server,
                &request(MAC, REQUEST, &[(OPT_SERVER_ID, &[10, 0, 0, 1])])
            )
            .is_none()
        );
        assert!(answer(&mut server, &[0u8; 10]).is_none());
    }
}
