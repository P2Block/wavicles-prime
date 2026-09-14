//! `prime.toml`. Keys are kebab-case to match the pool's existing install scripts.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    /// DATUM listener.
    #[serde(default = "d_listen")]
    pub listen: SocketAddr,
    /// HTTP listener for `/stats.json`, `/healthz`.
    #[serde(default = "d_stats_listen")]
    pub stats_listen: SocketAddr,
    /// What gateways should point at; only reported in stats.
    #[serde(default)]
    pub advertise_address: String,
    /// Ledger, key, block log, and exported `stats.json`/`ledger.json` live here.
    pub data_dir: PathBuf,
    /// Pool key file (64 hex bytes: ed25519 seed || x25519 secret). Generated if missing.
    #[serde(default)]
    pub key_file: Option<PathBuf>,
    #[serde(default = "d_motd")]
    pub motd: String,
    /// Gateway builds refused at the hello, as lowercase hex prefixes of the commit a stock
    /// gateway names in its user agent (`v0.4.1-beta/<commit>`). A refused gateway is sent
    /// `reject-gateway-message` as the MOTD of the server hello -- stock gateways log it as
    /// "DATUM Server MOTD" -- and the session closes before any work is exchanged. Empty
    /// refuses nothing. Why it exists (2026-09-14): the 2026-09-03 build (b9ea7dc3) miscounts
    /// the BLAKE2b coinbase bytes and truncates every split it is issued, so a block it found
    /// would leave most of the TIDES window unpaid while its own identity kept earning from
    /// everyone else's blocks. Fixed upstream on 2026-09-05 (7491a509).
    #[serde(default)]
    pub reject_gateway_builds: Vec<String>,
    /// Sent as the MOTD to a refused gateway. Keep it under 511 bytes: that is the gateway's
    /// MOTD buffer.
    #[serde(default = "d_reject_gateway_message")]
    pub reject_gateway_message: String,
    /// Address the pool's remainder and fee go to. Also the address gateways are configured with.
    pub payout_address: String,
    #[serde(default = "d_tag")]
    pub coinbase_tag: String,
    /// Identifies this Prime to gateways; nonzero.
    #[serde(default = "d_prime_id")]
    pub prime_id: u32,
    /// TIDES window in multiples of network difficulty.
    #[serde(default = "d_window")]
    pub window: u32,
    /// Floor on the window target, in difficulty-1 shares. Only matters where the network
    /// difficulty is tiny (regtest, fresh testnets); 0 disables.
    #[serde(default)]
    pub window_min_work: u64,
    /// Dust floor for split outputs, sats.
    #[serde(default = "d_min_payout")]
    pub min_payout: u64,
    #[serde(default)]
    pub fee_bps: u32,
    /// Public house-stratum fee. 0 means use `fee_bps` (same rate for everyone).
    #[serde(default)]
    pub stratum_fee_bps: u32,
    /// Gateway key prefixes (hex) that are the pool's own public stratum.
    #[serde(default)]
    pub house_gateways: Vec<String>,
    /// Treat loopback Prime connections as house stratum. Default on.
    #[serde(default = "d_true")]
    pub house_loopback: bool,
    /// Bytes of payout outputs a coinbaser may carry for house gateways (loopback or
    /// `house-gateways`) and for `big-coinbase-gateways`: the 16 KB coinbase class minus
    /// tags, extranonce, pool and witness outputs.
    #[serde(default = "d_coinbase_budget_house")]
    pub coinbase_budget_house: usize,
    /// The same for every other gateway. A stock gateway serves the ~730-byte coinbase class
    /// (about 16 outputs); anything past what its class holds is truncated by the gateway
    /// and its value lands in the pool output, so the split must stop before that.
    #[serde(default = "d_coinbase_budget_default")]
    pub coinbase_budget_default: usize,
    /// Gateway key prefixes (hex) known to run a 16 KB-coinbase build: they get the house
    /// budget without being house stratum (they still pay the DATUM fee).
    #[serde(default)]
    pub big_coinbase_gateways: Vec<String>,
    /// Smallest share difficulty gateways may send (power of two). Also sent as the vardiff floor.
    #[serde(default = "d_min_diff")]
    pub min_diff: u64,
    /// Slack per issued output when checking a coinbase, sats.
    #[serde(default = "d_tolerance")]
    pub split_tolerance: u64,
    /// Which network's addresses to accept: mainnet | testnet | signet | regtest.
    #[serde(default = "d_network")]
    pub network: String,
    /// Node JSON-RPC endpoint and credentials. Cookie wins if both are set.
    pub rpc: String,
    #[serde(default)]
    pub rpc_cookie: Option<PathBuf>,
    #[serde(default)]
    pub rpc_user: Option<String>,
    #[serde(default)]
    pub rpc_password: Option<String>,
    /// Node poll interval, seconds.
    #[serde(default = "d_poll")]
    pub poll: f64,
    /// Shares for a height this many blocks behind the tip are stale. 0 means only the
    /// current height. The default tolerates a template refresh in flight.
    #[serde(default = "d_stale_grace")]
    pub stale_grace_secs: u32,
    /// Import this legacy `ledger.json` once on first start (if the ledger is empty).
    #[serde(default)]
    pub import_ledger: Option<PathBuf>,
    /// Shown in stats.
    #[serde(default = "d_headline")]
    pub headline: String,
    /// Most DATUM sessions held open at once. A session buffers job and coinbase state on
    /// the gateway's behalf, so this bounds what an unknown key can make the pool hold.
    #[serde(default = "d_max_connections")]
    pub max_connections: u32,
    /// Most sessions from one remote address. A gateway is one connection; a farm is a few.
    #[serde(default = "d_max_connections_per_ip")]
    pub max_connections_per_ip: u32,
    /// Coinbase section bytes one session may have Prime hold across all of its job slots.
    /// A stock gateway's eight slots of seven ~16 KiB coinbase classes is under 1 MiB; the
    /// sixteen live slots Prime keeps at eight 20 000-byte sections each is 2.5 MiB.
    #[serde(default = "d_session_coinbase_budget")]
    pub session_coinbase_budget: usize,
    /// WAVICLES: commit a BLAKE2b-256 of the window snapshot the split was computed from as a
    /// zero-value OP_RETURN (first output of every coinbaser), and publish the snapshot.
    #[serde(default = "d_true")]
    pub commit_snapshot: bool,
    /// ASCII tag in front of the 32-byte hash inside the OP_RETURN (≤ 40 bytes).
    #[serde(default = "d_snapshot_tag")]
    pub snapshot_tag: String,
    /// WAVICLES: pay what earlier coinbases could not (dust, class limits, empty window) out of
    /// later coinbases instead of holding it as "owed".
    #[serde(default = "d_true")]
    pub carry_forward: bool,
    /// PyBLØCK: cap on payees per block; `value − fee` goes to the largest ones in full, so the
    /// pool's output is exactly its fee (no custody). 0 = unlimited (classic rule + carry).
    #[serde(default)]
    pub max_payees: usize,

    // Keys the previous Prime used. Accepted so an existing config starts unchanged;
    // `load` reports each one it saw.
    #[serde(default)]
    activation_height: Option<u32>,
    #[serde(default)]
    verify_shares: Option<String>,
    #[serde(default)]
    require_split_gateway: Option<bool>,
}

fn d_listen() -> SocketAddr {
    "0.0.0.0:28915".parse().unwrap()
}
fn d_stats_listen() -> SocketAddr {
    "127.0.0.1:28916".parse().unwrap()
}
fn d_reject_gateway_message() -> String {
    "This gateway build is refused: it miscounts the BLAKE2b coinbase bytes and truncates the \
     payout list, so a block it found would leave most of the window unpaid. Update your DATUM \
     gateway to a build from 2026-09-05 or later (commit 7491a509 onward) and reconnect; your \
     address keeps its standing in the window."
        .into()
}
fn d_motd() -> String {
    "Lazarus".into()
}
fn d_tag() -> String {
    "Lazarus".into()
}
fn d_prime_id() -> u32 {
    1
}
fn d_window() -> u32 {
    8
}
fn d_min_payout() -> u64 {
    546
}
fn d_min_diff() -> u64 {
    1
}
fn d_tolerance() -> u64 {
    2
}
fn d_network() -> String {
    "mainnet".into()
}
fn d_poll() -> f64 {
    0.5
}
fn d_stale_grace() -> u32 {
    30
}
fn d_headline() -> String {
    "Lazarus".into()
}
fn d_true() -> bool {
    true
}
fn d_coinbase_budget_house() -> usize {
    14_000 - 9 - 64
}
fn d_coinbase_budget_default() -> usize {
    540
}
fn d_max_connections() -> u32 {
    256
}
fn d_max_connections_per_ip() -> u32 {
    8
}
fn d_snapshot_tag() -> String {
    "PYBLOCK-TON618".into()
}
fn d_session_coinbase_budget() -> usize {
    4 << 20
}

impl Config {
    /// The commit a STOCK gateway names in its user agent, `v0.4.1-beta/<hex>`. `None` if the
    /// agent carries no commit, or carries one with a suffix (`<hex>+`, `<hex>-dirty`): that is
    /// a locally modified tree, and the commit alone no longer says what the binary does -- the
    /// house gateways are exactly that, b9ea7dc3 plus a local 164-byte header fix, and they pay
    /// every split. A modified build is neither refused nor trusted by commit; it is judged
    /// like any other by what its coinbases actually pay.
    pub fn ua_commit(ua: &str) -> Option<&str> {
        let (_, rest) = ua.rsplit_once('/')?;
        let hex = rest.trim();
        (hex.len() >= 7 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
    }
    /// The `reject-gateway-builds` prefix this user agent matches, if any.
    pub fn rejected_build(&self, ua: &str) -> Option<&str> {
        let commit = Self::ua_commit(ua)?.to_ascii_lowercase();
        self.reject_gateway_builds
            .iter()
            .map(String::as_str)
            .find(|p| !p.is_empty() && commit.starts_with(&p.to_ascii_lowercase()))
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut c: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if !c.min_diff.is_power_of_two() {
            return Err("min-diff must be a power of two".into());
        }
        if c.prime_id == 0 {
            return Err("prime-id must be nonzero".into());
        }
        if c.window == 0 {
            return Err("window must be at least 1".into());
        }
        if c.fee_bps > 10_000 {
            return Err("fee-bps cannot exceed 10000".into());
        }
        if c.stratum_fee_bps == 0 {
            c.stratum_fee_bps = c.fee_bps;
        }
        if c.stratum_fee_bps > 10_000 {
            return Err("stratum-fee-bps cannot exceed 10000".into());
        }
        for g in &mut c.house_gateways {
            *g = g.to_ascii_lowercase();
        }
        for g in &mut c.big_coinbase_gateways {
            *g = g.to_ascii_lowercase();
        }
        if c.coinbase_budget_default < 64 || c.coinbase_budget_house < c.coinbase_budget_default {
            return Err("coinbase-budget-default must be at least 64 and coinbase-budget-house at least coinbase-budget-default".into());
        }
        if c.snapshot_tag.len() > 40 || c.snapshot_tag.bytes().any(|b| !(0x20..0x7f).contains(&b)) {
            return Err("snapshot-tag must be printable ASCII of at most 40 bytes".into());
        }
        if c.coinbase_tag.len() > 32 {
            return Err("coinbase-tag is too long (32 bytes max)".into());
        }
        if c.max_connections == 0 || c.max_connections_per_ip == 0 {
            return Err("max-connections and max-connections-per-ip must be at least 1".into());
        }
        if c.session_coinbase_budget < 64 * 1024 {
            return Err("session-coinbase-budget must be at least 65536 bytes (one huge coinbase class)".into());
        }
        if c.key_file.is_none() {
            // A data dir left by lazarus-prime keeps its identity: same key file, same pubkey.
            let ours = c.data_dir.join("prime.key");
            let legacy = c.data_dir.join("lazarus-prime.key");
            c.key_file = Some(if !ours.exists() && legacy.exists() { legacy } else { ours });
        }
        Ok(c)
    }

    /// One line per legacy key present in the file, explaining why it no longer applies.
    pub fn legacy_notes(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.activation_height.is_some() {
            v.push("activation-height is ignored: every share is verified as BLAKE2b header v2; SHA256d shares are rejected as bad-version".into());
        }
        if let Some(mode) = &self.verify_shares {
            v.push(format!("verify-shares = {mode:?} is ignored: shares are always verified and the coinbase is always checked against the issued TIDES split"));
        }
        for p in &self.reject_gateway_builds {
            let ok = p.len() >= 7 && p.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
            if !ok {
                v.push(format!("reject-gateway-builds entry {p:?} is not a lowercase hex commit prefix of at least 7 chars; it will never match"));
            }
        }
        if self.reject_gateway_message.len() > 511 {
            v.push("reject-gateway-message is longer than the gateway's 511-byte MOTD buffer; it will be cut".into());
        }
        if self.require_split_gateway.is_some() {
            v.push("require-split-gateway is ignored: stock DATUM gateways pay the split from the coinbaser reply, so none need a patched user agent".into());
        }
        v
    }

    pub fn key_file(&self) -> &Path {
        self.key_file.as_deref().unwrap()
    }

    pub fn min_pot(&self) -> u8 {
        self.min_diff.trailing_zeros() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_only_the_listed_build() {
        let mut c: Config = toml::from_str(LEGACY).unwrap();
        c.reject_gateway_builds = vec!["b9ea7dc3".into()];
        let bad = "v0.4.1-beta/b9ea7dc3eb91352565ab487ec55ed6ee5964a440";
        let good = "v0.4.1-beta/7491a5099dd5d887a027c812f71d";
        assert_eq!(c.rejected_build(bad), Some("b9ea7dc3"));
        assert_eq!(c.rejected_build(good), None);
        assert_eq!(c.rejected_build("v0.4.1-beta/B9EA7DC3EB91"), Some("b9ea7dc3"), "case-insensitive on the agent");
        assert_eq!(c.rejected_build("no-commit-here"), None);
        assert_eq!(Config::ua_commit("v0.4.1-beta/"), None);
        assert_eq!(c.rejected_build("v0.4.1-beta/b9ea7dc3eb91352565ab487ec55ed6ee5964a440+"), None,
            "a modified tree (+) is not judged by its base commit: the house gateways are one");
        assert_eq!(c.rejected_build("v0.4.1-beta/b9ea7dc3eb91-dirty"), None);
        c.reject_gateway_builds.clear();
        assert_eq!(c.rejected_build(bad), None, "empty list refuses nothing");
        assert!(d_reject_gateway_message().len() < 511);
    }

    /// The file the previous Prime shipped, minus the placeholder address.
    const LEGACY: &str = r#"
listen = "0.0.0.0:28915"
stats-listen = "127.0.0.1:28916"
advertise-address = "stratum.awokenlazarus.xyz:28915"
data-dir = "/home/umbrel/blake2b/lazarus-prime"
motd = "Lazarus"
min-diff = 1
payout-address = "bc1qt5praystcdle0nq04e3h02yjszha82uzhww85x6972lcy40k4eyqz9jfaq"
coinbase-tag = "Lazarus"
prime-id = 1
window = 8
min-payout = 546
fee-bps = 50
activation-height = 961640
headline = "Lazarus"
rpc = "http://127.0.0.1:9332"
rpc-cookie = "/home/umbrel/umbrel/app-data/bitcoin-knots/data/bitcoin/.cookie"
poll = 0.5
verify-shares = "enforce"
require-split-gateway = true
"#;

    #[test]
    fn legacy_prime_toml_loads_unchanged() {
        let dir = std::env::temp_dir().join(format!("primed-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("prime.toml");
        let text = LEGACY.replace("/home/umbrel/blake2b/lazarus-prime", dir.to_str().unwrap());
        std::fs::write(&p, &text).unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.listen.port(), 28915);
        assert_eq!(c.fee_bps, 50);
        assert_eq!(c.stratum_fee_bps, 50);
        assert!(c.house_loopback);
        assert_eq!(c.window, 8);
        assert_eq!(c.key_file(), dir.join("prime.key"));
        assert_eq!(c.min_pot(), 0);
        assert_eq!(c.legacy_notes().len(), 3);
        // a data dir the old Prime left behind keeps its key, hence its pubkey
        std::fs::write(dir.join("lazarus-prime.key"), "00").unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.key_file(), dir.join("lazarus-prime.key"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_keys_are_still_errors() {
        let dir = std::env::temp_dir().join(format!("primed-cfg-u-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("prime.toml");
        std::fs::write(&p, format!("{LEGACY}\nfee_percent = 1\n")).unwrap();
        let r = Config::load(&p);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(r.unwrap_err().contains("fee_percent"));
    }
}
