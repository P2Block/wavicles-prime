//! P2Block runtime controls: `<data-dir>/controls.json`, written by the pool's control plane and
//! re-read by Prime every few seconds, so fee changes, per-identity fee overrides and bans take
//! effect without a restart. Missing file = no controls. A file that fails to parse keeps the
//! previous controls in force (never fail open, never fail to "everyone banned").
//!
//! ```json
//! { "fee_bps": 100, "stratum_fee_bps": 200,
//!   "fee_overrides": { "bc1q…": 50 },
//!   "banned_identities": ["bc1q…"],
//!   "banned_ips": ["203.0.113.9", "198.51.100.0/24"],
//!   "updated": "2026-09-14T22:00:00Z" }
//! ```
use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;

use crate::address;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ControlsFile {
    pub fee_bps: Option<u32>,
    pub stratum_fee_bps: Option<u32>,
    pub fee_overrides: BTreeMap<String, u32>,
    pub banned_identities: Vec<String>,
    pub banned_ips: Vec<String>,
    pub updated: Option<String>,
}

/// Parsed, validated controls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Controls {
    pub fee_bps: Option<u32>,
    pub stratum_fee_bps: Option<u32>,
    /// Canonical identity → bps.
    pub fee_overrides: BTreeMap<String, u32>,
    pub banned_identities: BTreeSet<String>,
    /// (network address, prefix bits); a bare IP is /32 or /128.
    pub banned_nets: Vec<(IpAddr, u8)>,
    pub updated: Option<String>,
    /// mtime of the file this came from, to skip re-reading an unchanged file.
    pub mtime: Option<SystemTime>,
}

impl Controls {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("controls.json")
    }

    /// `Ok(None)` when the file does not exist. Validation errors are returned; the caller keeps
    /// the previous controls.
    pub fn load(path: &Path) -> Result<Option<Controls>, String> {
        let md = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let f: ControlsFile = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut c = Controls {
            fee_bps: f.fee_bps,
            stratum_fee_bps: f.stratum_fee_bps,
            updated: f.updated,
            mtime: md.modified().ok(),
            ..Default::default()
        };
        for bps in [c.fee_bps, c.stratum_fee_bps].into_iter().flatten() {
            if bps > 10_000 {
                return Err(format!("fee {bps} bps is over 100%"));
            }
        }
        for (id, bps) in f.fee_overrides {
            if bps > 10_000 {
                return Err(format!("fee override for {id}: {bps} bps is over 100%"));
            }
            c.fee_overrides.insert(address::canonical_identity(address::identity_of(&id)), bps);
        }
        for id in f.banned_identities {
            c.banned_identities.insert(address::canonical_identity(address::identity_of(&id)));
        }
        for s in f.banned_ips {
            c.banned_nets.push(parse_net(&s).ok_or_else(|| format!("banned_ips: {s:?} is not an IP or CIDR"))?);
        }
        Ok(Some(c))
    }

    pub fn ip_banned(&self, ip: IpAddr) -> bool {
        self.banned_nets.iter().any(|(net, bits)| in_net(ip, *net, *bits))
    }

    pub fn identity_banned(&self, identity: &str) -> bool {
        self.banned_identities.contains(identity)
    }

    pub fn summary(&self) -> String {
        format!(
            "fee={} stratum_fee={} overrides={} banned_identities={} banned_nets={}{}",
            self.fee_bps.map(|b| b.to_string()).unwrap_or_else(|| "config".into()),
            self.stratum_fee_bps.map(|b| b.to_string()).unwrap_or_else(|| "config".into()),
            self.fee_overrides.len(),
            self.banned_identities.len(),
            self.banned_nets.len(),
            self.updated.as_deref().map(|u| format!(" updated={u}")).unwrap_or_default()
        )
    }
}

fn parse_net(s: &str) -> Option<(IpAddr, u8)> {
    let (ip, bits) = match s.split_once('/') {
        Some((a, b)) => (a.parse::<IpAddr>().ok()?, b.parse::<u8>().ok()?),
        None => {
            let ip = s.parse::<IpAddr>().ok()?;
            (ip, if ip.is_ipv4() { 32 } else { 128 })
        }
    };
    let max = if ip.is_ipv4() { 32 } else { 128 };
    (bits <= max).then_some((ip, bits))
}

fn in_net(ip: IpAddr, net: IpAddr, bits: u8) -> bool {
    match (ip, net) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - u32::from(bits)) };
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - u32::from(bits)) };
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nets_match_hosts_and_prefixes() {
        let c = Controls {
            banned_nets: vec![
                parse_net("203.0.113.9").unwrap(),
                parse_net("198.51.100.0/24").unwrap(),
                parse_net("2001:db8::/32").unwrap(),
            ],
            ..Default::default()
        };
        assert!(c.ip_banned("203.0.113.9".parse().unwrap()));
        assert!(!c.ip_banned("203.0.113.10".parse().unwrap()));
        assert!(c.ip_banned("198.51.100.77".parse().unwrap()));
        assert!(!c.ip_banned("198.51.101.1".parse().unwrap()));
        assert!(c.ip_banned("2001:db8:1::5".parse().unwrap()));
        assert!(parse_net("300.1.1.1").is_none());
        assert!(parse_net("10.0.0.0/33").is_none());
    }

    #[test]
    fn file_round_trip_and_validation() {
        let dir = std::env::temp_dir().join(format!("p2block-controls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = Controls::path(&dir);
        assert!(Controls::load(&p).unwrap().is_none(), "missing file = no controls");
        std::fs::write(&p, r#"{"fee_bps":150,"fee_overrides":{"bc1qtest.worker":50},"banned_identities":["bc1qbad"],"banned_ips":["10.0.0.0/8"],"updated":"now"}"#).unwrap();
        let c = Controls::load(&p).unwrap().unwrap();
        assert_eq!(c.fee_bps, Some(150));
        assert_eq!(c.fee_overrides.get("bc1qtest"), Some(&50), "override keys drop any .worker suffix");
        assert!(c.identity_banned("bc1qbad"));
        assert!(c.ip_banned("10.3.4.5".parse().unwrap()));
        std::fs::write(&p, r#"{"fee_bps":20000}"#).unwrap();
        assert!(Controls::load(&p).is_err(), "over 100% is rejected");
        std::fs::write(&p, r#"{"nope":1}"#).unwrap();
        assert!(Controls::load(&p).is_err(), "unknown keys are rejected (typo protection)");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
