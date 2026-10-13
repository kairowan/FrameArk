//! Explicit out-of-band bootstrap; never discovery or automatic trust.
use std::{fmt, net::SocketAddrV4};

use ring::digest::{SHA256, digest};

/// Maximum UTF-8 bytes in a manually exchanged pairing ticket.
pub const MAX_PAIRING_TICKET_BYTES: usize = 8_256;
const VERSION: &str = "frameark-pair-v1";
const MAX_CERTIFICATE: usize = 4_096;

/// A private-LAN endpoint and ephemeral public TLS certificate, without a PIN.
#[derive(Clone, Eq, PartialEq)]
pub struct PairingTicket {
    address: SocketAddrV4,
    certificate: Vec<u8>,
}

impl fmt::Debug for PairingTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingTicket([REDACTED])")
    }
}

/// Invalid version, endpoint, certificate length, or canonical encoding.
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidPairingTicket;

impl fmt::Display for InvalidPairingTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid local pairing ticket")
    }
}
impl std::error::Error for InvalidPairingTicket {}

impl PairingTicket {
    /// Constructs a ticket restricted to RFC1918 or link-local IPv4, nonzero port.
    /// TLS certificate validity is additionally checked by the pinned TLS client.
    pub fn new(address: SocketAddrV4, certificate: Vec<u8>) -> Result<Self, InvalidPairingTicket> {
        if !(address.ip().is_private() || address.ip().is_link_local())
            || address.port() == 0
            || certificate.is_empty()
            || certificate.len() > MAX_CERTIFICATE
        {
            return Err(InvalidPairingTicket);
        }
        Ok(Self {
            address,
            certificate,
        })
    }

    /// Parses exactly three canonical ASCII lines. No DNS, URLs, PIN, or extensions.
    pub fn parse(text: &str) -> Result<Self, InvalidPairingTicket> {
        if text.len() > MAX_PAIRING_TICKET_BYTES || !text.is_ascii() {
            return Err(InvalidPairingTicket);
        }
        let mut lines = text.split('\n');
        if lines.next() != Some(VERSION) {
            return Err(InvalidPairingTicket);
        }
        let endpoint = lines.next().ok_or(InvalidPairingTicket)?;
        let address: SocketAddrV4 = endpoint.parse().map_err(|_| InvalidPairingTicket)?;
        if address.to_string() != endpoint {
            return Err(InvalidPairingTicket);
        }
        let hex = lines.next().ok_or(InvalidPairingTicket)?;
        if lines.next().is_some() || hex.len() > MAX_CERTIFICATE * 2 || !hex.len().is_multiple_of(2)
        {
            return Err(InvalidPairingTicket);
        }
        let nibble = |byte| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(InvalidPairingTicket),
        };
        let certificate = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| Ok(nibble(pair[0])? * 16 + nibble(pair[1])?))
            .collect::<Result<Vec<_>, InvalidPairingTicket>>()?;
        Self::new(address, certificate)
    }

    /// Serializes for explicit user sharing; contains local network metadata.
    pub fn encode(&self) -> String {
        format!("{VERSION}\n{}\n{}", self.address, hex(&self.certificate))
    }

    /// Explicit numeric local address, never resolved through DNS.
    pub const fn address(&self) -> SocketAddrV4 {
        self.address
    }
    /// Public certificate pin used with the fixed `frameark.local` TLS name.
    pub fn certificate(&self) -> &[u8] {
        &self.certificate
    }
    /// Full lowercase SHA-256 of the ephemeral certificate (not device identity).
    pub fn fingerprint(&self) -> String {
        hex(digest(&SHA256, &self.certificate).as_ref())
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                DIGITS[(b >> 4) as usize] as char,
                DIGITS[(b & 15) as usize] as char,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_boundaries_and_redaction() {
        for address in [
            "10.0.2.2:65535",
            "192.168.1.1:1",
            "172.16.0.1:1234",
            "169.254.1.2:5000",
        ] {
            let ticket =
                PairingTicket::new(address.parse().unwrap(), vec![42; MAX_CERTIFICATE]).unwrap();
            assert_eq!(PairingTicket::parse(&ticket.encode()).unwrap(), ticket);
            assert!(ticket.encode().len() <= MAX_PAIRING_TICKET_BYTES);
            assert_eq!(ticket.fingerprint().len(), 64);
            assert_eq!(format!("{ticket:?}"), "PairingTicket([REDACTED])");
        }
    }

    #[test]
    fn rejects_unsafe_endpoints_and_malformed_tickets() {
        for address in [
            "127.0.0.1:1",
            "0.0.0.0:1",
            "8.8.8.8:443",
            "224.0.0.1:1",
            "192.168.1.1:0",
            "172.32.0.1:1",
        ] {
            assert!(PairingTicket::new(address.parse().unwrap(), vec![1]).is_err());
        }
        let good = PairingTicket::new("192.168.1.1:1234".parse().unwrap(), vec![0xab])
            .unwrap()
            .encode();
        for bad in [
            good.replace("v1", "v2"),
            good.replace("ab", "AB"),
            format!("{good}\n"),
            good.replace("ab", "a"),
            good.replace("ab", "zz"),
            good.replace("ab", ""),
            good.replace("192.168.1.1", "localhost"),
            good.replace("\n", "\r\n"),
            good.replace("ab", &"aa".repeat(MAX_CERTIFICATE + 1)),
        ] {
            assert!(PairingTicket::parse(&bad).is_err());
        }
        for index in 0..good.len() {
            assert!(PairingTicket::parse(&good[..index]).is_err());
        }
    }
}
