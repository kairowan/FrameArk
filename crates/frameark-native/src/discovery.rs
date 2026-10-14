//! Experimental, unauthenticated DNS-SD bootstrap hints. A hint is not trust:
//! users must compare the full TLS fingerprint before submitting a PIN.
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddrV4};
use std::time::{Duration, Instant};

use frameark_discovery::{
    DiscoveredService, DiscoveryError, DiscoveryEvent, FRAMEARK_SERVICE_TYPE, PublishedService,
};
use frameark_transport::PairingTicket;

const MAX_CERT_BYTES: usize = 512;
const CHUNK: usize = 240;
const MAX_PROPERTIES: usize = 16;
const MAX_TXT_BYTES: usize = 1300;
const MAX_SERVICES: usize = 8;
const MAX_ADDRESSES: usize = 4;
/// Application cache lifetime; goodbye/invalid updates remove entries earlier.
pub const HINT_LIFETIME: Duration = Duration::from_secs(30);

/// Adds a small public ephemeral TLS certificate, never a PIN or private key.
/// Certificates larger than 512 bytes retain the manual-ticket path instead.
pub fn advertise_certificate(
    mut service: PublishedService,
    certificate: &[u8],
) -> Result<PublishedService, DiscoveryError> {
    if certificate.is_empty() || certificate.len() > MAX_CERT_BYTES {
        return Err(DiscoveryError::InvalidField {
            field: "bootstrap_certificate",
            reason: "length",
        });
    }
    let hex: String = certificate
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    service = service
        .with_property("boot", "cert1")?
        .with_property("certs", hex.len().div_ceil(CHUNK).to_string())?;
    for (index, chunk) in hex.as_bytes().chunks(CHUNK).enumerate() {
        // Hex is ASCII, and chunk boundaries cannot split a character.
        let value = std::str::from_utf8(chunk).map_err(|_| DiscoveryError::InvalidField {
            field: "bootstrap_certificate",
            reason: "encoding",
        })?;
        service = service.with_property(format!("cert{index}"), value)?;
    }
    Ok(service)
}

/// Produces at most four local IPv4 tickets. No hostname lookup, socket, or
/// authentication occurs here. Unknown optional TXT keys are bounded/ignored.
pub fn candidate_tickets(service: &DiscoveredService) -> Vec<PairingTicket> {
    parse_candidates(service).unwrap_or_default()
}

fn parse_candidates(service: &DiscoveredService) -> Option<Vec<PairingTicket>> {
    if !service
        .service_type
        .eq_ignore_ascii_case(FRAMEARK_SERVICE_TYPE)
        || service.fullname.len() > 255
        || !service
            .fullname
            .to_ascii_lowercase()
            .ends_with(&format!(".{FRAMEARK_SERVICE_TYPE}"))
        || service.properties.len() > MAX_PROPERTIES
        || service.addresses.len() > 16
        || service.properties.iter().any(|(key, value)| {
            key.is_empty() || !key.is_ascii() || key.len() > 63 || key.len() + value.len() + 1 > 255
        })
        || service
            .properties
            .iter()
            .map(|(key, value)| key.len() + value.len() + 2)
            .sum::<usize>()
            > MAX_TXT_BYTES
    {
        return None;
    }
    let properties: BTreeMap<_, _> = service
        .properties
        .iter()
        .map(|(key, value)| (key.to_ascii_lowercase(), value.as_str()))
        .collect();
    // DNS-SD keys are case-insensitive; ambiguous duplicates fail closed.
    if properties.len() != service.properties.len()
        || properties.get("proto") != Some(&"fanp1")
        || properties.get("ver") != Some(&"0.1")
        || properties.get("tls") != Some(&"pin")
        || properties.get("boot") != Some(&"cert1")
    {
        return None;
    }
    let count = match *properties.get("certs")? {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        _ => return None,
    };
    if properties.keys().any(|key| {
        key.starts_with("cert")
            && key != "certs"
            && !(0..count).any(|index| key == &format!("cert{index}"))
    }) {
        return None;
    }
    let mut hex = String::new();
    for index in 0..count {
        let part = *properties.get(&format!("cert{index}"))?;
        if part.is_empty()
            || part.len() > CHUNK
            || !part.len().is_multiple_of(2)
            || (index + 1 < count && part.len() != CHUNK)
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        hex.push_str(part);
    }
    if hex.len() > MAX_CERT_BYTES * 2 {
        return None;
    }
    Some(
        service
            .addresses
            .iter()
            .filter_map(|ip| {
                let IpAddr::V4(ip) = ip else {
                    return None;
                };
                PairingTicket::parse(&format!(
                    "frameark-pair-v1\n{}\n{hex}",
                    SocketAddrV4::new(*ip, service.port)
                ))
                .ok()
            })
            .take(MAX_ADDRESSES)
            .collect(),
    )
}

/// Bounded, nonpersistent candidate cache. Identity/session trust stays in the
/// existing pairing path; service names are never displayed as verified names.
#[derive(Default)]
pub struct CandidateCache {
    entries: BTreeMap<String, (Instant, Vec<PairingTicket>)>,
}
impl CandidateCache {
    /// Applies a backend event, invalidating changed/unsupported advertisements.
    pub fn update(&mut self, event: DiscoveryEvent, now: Instant) {
        self.expire(now);
        match event {
            DiscoveryEvent::ServiceResolved(service) => {
                let tickets = candidate_tickets(&service);
                if tickets.is_empty() {
                    self.entries.remove(&service.fullname);
                } else if self.entries.contains_key(&service.fullname)
                    || self.entries.len() < MAX_SERVICES
                {
                    self.entries.insert(service.fullname, (now, tickets));
                }
            }
            DiscoveryEvent::ServiceRemoved { fullname } => {
                self.entries.remove(&fullname);
            }
            _ => {}
        }
    }
    /// Returns at most 32 independent copies; expired entries cannot be selected.
    pub fn snapshot(&mut self, now: Instant) -> Vec<PairingTicket> {
        self.expire(now);
        let mut tickets = Vec::new();
        for (_, values) in self.entries.values() {
            for ticket in values {
                if !tickets.contains(ticket) {
                    tickets.push(ticket.clone());
                }
            }
        }
        tickets
    }
    fn expire(&mut self, now: Instant) {
        self.entries
            .retain(|_, (seen, _)| now.saturating_duration_since(*seen) < HINT_LIFETIME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn service(certificate: &[u8]) -> DiscoveredService {
        let published = advertise_certificate(
            PublishedService::new("Receiver", "frameark.local.", 54321)
                .unwrap()
                .with_property("proto", "fanp1")
                .unwrap()
                .with_property("ver", "0.1")
                .unwrap()
                .with_property("tls", "pin")
                .unwrap(),
            certificate,
        )
        .unwrap();
        DiscoveredService {
            service_type: FRAMEARK_SERVICE_TYPE.into(),
            fullname: format!("Receiver.{FRAMEARK_SERVICE_TYPE}"),
            host: "never-resolved.invalid".into(),
            port: 54321,
            addresses: ["192.168.1.23".parse().unwrap()].into(),
            properties: published.properties().clone(),
        }
    }
    #[test]
    fn certificate_boundaries_roundtrip_and_no_secrets() {
        for size in [1, 120, 121, 240, 360, 480, 512] {
            let cert = vec![0xab; size];
            let candidate = service(&cert);
            let tickets = candidate_tickets(&candidate);
            assert_eq!(tickets.len(), 1);
            assert_eq!(tickets[0].certificate(), cert);
            assert!(
                candidate
                    .properties
                    .keys()
                    .all(|key| !key.contains("pin") && !key.contains("key"))
            );
        }
        for cert in [vec![], vec![0; 513]] {
            assert!(
                advertise_certificate(PublishedService::new("x", "x.local.", 1).unwrap(), &cert)
                    .is_err()
            );
        }
    }
    #[test]
    fn malformed_version_chunks_and_oversized_inputs_fail_closed() {
        for (key, value) in [
            ("proto", "fanp2"),
            ("ver", "0.2"),
            ("tls", "none"),
            ("boot", "cert2"),
            ("certs", "00"),
            ("certs", "6"),
            ("certs", "1"),
            ("cert0", "AB"),
            ("cert0", "aa"),
            ("cert1", "a"),
            ("cert2", "aa"),
            ("CERT0", "aa"),
            ("unknown", &"x".repeat(256)),
        ] {
            let mut bad = service(&[0xab; 200]);
            bad.properties.insert(key.into(), value.into());
            assert!(candidate_tickets(&bad).is_empty(), "{key}");
        }
        for key in ["proto", "ver", "tls", "boot", "certs", "cert0", "cert1"] {
            let mut bad = service(&[0xab; 200]);
            bad.properties.remove(key);
            assert!(candidate_tickets(&bad).is_empty());
        }
        let mut bad = service(&[0xab; 512]);
        bad.properties.insert("cert4".into(), "aa".repeat(120));
        assert!(candidate_tickets(&bad).is_empty());
        let mut bad = service(&[1]);
        bad.service_type = "_other._udp.local.".into();
        assert!(candidate_tickets(&bad).is_empty());
        bad = service(&[1]);
        bad.fullname = "x".repeat(256);
        assert!(candidate_tickets(&bad).is_empty());
        bad = service(&[1]);
        bad.port = 0;
        assert!(candidate_tickets(&bad).is_empty());
        bad = service(&[1]);
        for i in 0..17 {
            bad.properties.insert(format!("extra{i}"), "x".into());
        }
        assert!(candidate_tickets(&bad).is_empty());
    }
    #[test]
    fn only_local_ipv4_and_bounded_addresses() {
        let mut value = service(&[1]);
        value.addresses = [
            "127.0.0.1",
            "0.0.0.0",
            "8.8.8.8",
            "224.0.0.1",
            "::1",
            "fe80::1",
        ]
        .map(|s| s.parse().unwrap())
        .into();
        assert!(candidate_tickets(&value).is_empty());
        value.addresses = (1..=16)
            .map(|i| IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, i)))
            .collect::<BTreeSet<_>>();
        assert_eq!(candidate_tickets(&value).len(), 4);
        value.addresses.insert("10.1.1.1".parse().unwrap());
        assert!(candidate_tickets(&value).is_empty());
    }

    #[test]
    fn optional_keys_case_and_total_txt_budget() {
        let mut value = service(&[42; 512]);
        value.properties = value
            .properties
            .into_iter()
            .map(|(key, value)| (key.to_uppercase(), value))
            .collect();
        value.properties.insert("OPTIONAL".into(), "ignored".into());
        assert_eq!(candidate_tickets(&value).len(), 1);
        value.properties.insert("EXTRA1".into(), "x".repeat(240));
        value.properties.insert("EXTRA2".into(), "x".repeat(240));
        assert!(candidate_tickets(&value).is_empty());
    }

    #[test]
    fn arbitrary_txt_byte_mutations_remain_bounded() {
        for index in 0..256u16 {
            for size in [0, 1, 2, 239, 240, 241, 513, 1301] {
                let mut value = service(&[1; 121]);
                value.properties.insert(
                    "cert0".into(),
                    char::from_u32(u32::from(index))
                        .unwrap()
                        .to_string()
                        .repeat(size),
                );
                assert!(candidate_tickets(&value).len() <= MAX_ADDRESSES);
            }
        }
    }
    #[test]
    fn cache_expiry_goodbye_replacement_and_capacity() {
        let now = Instant::now();
        let mut cache = CandidateCache::default();
        for i in 0..100 {
            let mut value = service(&[i]);
            value.fullname = format!("receiver{i}.{FRAMEARK_SERVICE_TYPE}");
            cache.update(DiscoveryEvent::ServiceResolved(value), now);
        }
        assert_eq!(cache.snapshot(now).len(), MAX_SERVICES);
        cache.update(
            DiscoveryEvent::ServiceRemoved {
                fullname: format!("receiver0.{FRAMEARK_SERVICE_TYPE}"),
            },
            now,
        );
        assert_eq!(cache.snapshot(now).len(), MAX_SERVICES - 1);
        let mut bad = service(&[99]);
        bad.fullname = format!("receiver1.{FRAMEARK_SERVICE_TYPE}");
        bad.properties.remove("boot");
        cache.update(DiscoveryEvent::ServiceResolved(bad), now);
        assert_eq!(cache.snapshot(now).len(), MAX_SERVICES - 2);
        assert!(cache.snapshot(now + HINT_LIFETIME).is_empty());
        let value = service(&[1]);
        cache.update(DiscoveryEvent::ServiceResolved(value.clone()), now);
        let mut updated = value;
        updated.properties.insert("cert0".into(), "02".into());
        cache.update(DiscoveryEvent::ServiceResolved(updated), now);
        assert_eq!(cache.snapshot(now)[0].certificate(), &[2]);
    }
}
