//! Node poller: tracks the tip and difficulty, sizes the TIDES window, relays new-block
//! notifications, and confirms found blocks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::state::{now, Shared, Tip};

pub async fn run(shared: Arc<Shared>) {
    let period = Duration::from_secs_f64(shared.cfg.poll.max(0.2));
    let mut confirm_at = Instant::now();
    let mut warned = false;
    loop {
        match shared.rpc.getblockchaininfo().await {
            Ok(info) => {
                if warned {
                    log::info!("node is back");
                    warned = false;
                }
                let height = info.get("blocks").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let hash = info.get("bestblockhash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let difficulty = node_difficulty(&info);
                let current = shared.tip_snapshot();
                let changed = current.as_ref().is_none_or(|t| t.hash != hash);
                if changed {
                    let tip = Tip { height, hash: hash.clone(), difficulty, seen_at: Instant::now(), seen_ts: now() };
                    log::info!("tip height={height} hash={} difficulty={difficulty:.3}", &hash[..hash.len().min(16)]);
                    shared.tip_tx.send_replace(Some(tip));
                    if current.is_some() {
                        let _ = shared.notify.send(0);
                    }
                }
                if difficulty > 0.0 {
                    let target = shared.window_target(difficulty);
                    let mut ledger = shared.ledger.lock().unwrap();
                    if ledger.window.target_work() != target {
                        log::info!("TIDES window target -> {target} ({}x difficulty)", shared.cfg.window);
                        ledger.set_target(target);
                        if let Err(e) = ledger.persist_window() {
                            log::error!("ledger persist after target change failed: {e}");
                        }
                    }
                }
                if confirm_at <= Instant::now() {
                    confirm_at = Instant::now() + Duration::from_secs(30);
                    confirm_blocks(&shared, height).await;
                }
            }
            Err(e) => {
                if !warned {
                    log::warn!("node rpc failed: {e} (shares are still accepted without a staleness check)");
                    warned = true;
                }
            }
        }
        tokio::time::sleep(period).await;
    }
}

/// How many blocks past a candidate we keep re-checking one we have called an orphan. A block
/// found on a gateway whose node lagged the pool node by one is a competing tip until the next
/// block lands; if that lands on the gateway's branch, the "orphan" is main chain after all.
const ORPHAN_RECHECK_BLOCKS: u32 = 100;

/// Mark recorded blocks settled once the node has them in the main chain.
async fn confirm_blocks(shared: &Shared, tip_height: u32) {
    let pending: Vec<(String, u32)> = shared
        .blocks
        .lock()
        .unwrap()
        .iter()
        .rev()
        .filter(|b| {
            if b.settled {
                return false;
            }
            if b.kind.starts_with("orphan") {
                b.height + ORPHAN_RECHECK_BLOCKS > tip_height
            } else {
                b.height + 2000 > tip_height
            }
        })
        .take(20)
        .map(|b| (b.hash.clone(), b.height))
        .collect();
    for (hash, height) in pending {
        match shared.rpc.getblockheader(&hash).await {
            Ok(h) => {
                let conf = h.get("confirmations").and_then(|v| v.as_i64()).unwrap_or(0);
                if conf > 0 {
                    log::info!("block {hash} at {height} confirmed ({conf})");
                    shared.update_block(&hash, |r| {
                        if let Some(kind) = r.kind.strip_prefix("orphan:") {
                            log::info!("block {} at {} is back in the main chain", r.hash, r.height);
                            r.kind = kind.to_string();
                        }
                        r.settled = true;
                    });
                    shared.apply_carry_for_settled(&hash);
                } else if conf < 0 {
                    mark_orphan(shared, &hash, height, "is not in the main chain");
                }
            }
            Err(crate::rpc::RpcError::Node { code: -5, .. }) => {
                // unknown to the node yet; if the chain has moved well past it, it lost
                if tip_height > height + 6 {
                    mark_orphan(shared, &hash, height, "never reached the node");
                }
            }
            Err(e) => log::debug!("getblockheader {hash}: {e}"),
        }
    }
}

/// Label a recorded block as orphaned, once. Orphans stay unsettled so `confirm_blocks` keeps
/// re-checking them for a while; this must not stack a prefix per pass.
fn mark_orphan(shared: &Shared, hash: &str, height: u32, why: &str) {
    let already = shared
        .blocks
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|r| r.hash == hash)
        .map_or(true, |r| r.kind.starts_with("orphan:"));
    if already {
        return;
    }
    log::warn!("block {hash} at {height} {why}");
    shared.update_block(hash, |r| r.kind = format!("orphan:{}", r.kind));
}

/// Difficulty 1 measured the node's way: the target of bits 0x1d00ffff, `0xffff * 2^208`.
fn diff1_target() -> f64 {
    65535.0 * 2f64.powi(208)
}

/// The tip difficulty in the unit `getblockchaininfo.difficulty` used before Knots 29.4.2.
/// 29.4.2 omits `difficulty` on a BLAKE2b (header v2) tip and prints `difficulty_blake2b`,
/// the expected hash count, instead. Read `difficulty` verbatim, else derive it from `bits`
/// (present on every version), else `difficulty_blake2b / 2^32` scaled to the same unit.
/// Zero when none of the three is usable; the caller keeps the previous window target then.
pub fn node_difficulty(info: &serde_json::Value) -> f64 {
    if let Some(d) = info.get("difficulty").and_then(|v| v.as_f64()) {
        if d > 0.0 {
            return d;
        }
    }
    if let Some(d) = info.get("bits").and_then(|v| v.as_str()).and_then(difficulty_from_bits) {
        return d;
    }
    if let Some(work) = info.get("difficulty_blake2b").and_then(|v| v.as_f64()) {
        if work > 0.0 {
            return work / 4294967296.0 * (65535.0 / 65536.0);
        }
    }
    log::warn!("node reports no difficulty, bits or difficulty_blake2b; keeping the previous window target");
    0.0
}

fn difficulty_from_bits(hex: &str) -> Option<f64> {
    let bits = u32::from_str_radix(hex, 16).ok()?;
    let exp = (bits >> 24) as i32;
    let mantissa = f64::from(bits & 0x00ff_ffff);
    if mantissa == 0.0 || bits & 0x0080_0000 != 0 || !(1..=32).contains(&exp) {
        return None;
    }
    let target = mantissa * 256f64.powi(exp - 3);
    let d = diff1_target() / target;
    (d.is_finite() && d > 0.0).then_some(d)
}

#[cfg(test)]
mod difficulty_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_field_is_read_verbatim_when_present() {
        let v = json!({ "difficulty": 3417233412.773893, "bits": "190141c0" });
        assert_eq!(node_difficulty(&v), 3417233412.773893);
    }

    #[test]
    fn knots_29_4_2_tip_is_derived_from_bits_within_a_hundredth_of_a_percent() {
        // 29.4.2 at bits 190141c0: no "difficulty", difficulty_blake2b = GetBlockProof
        let v = json!({ "bits": "190141c0", "difficulty_blake2b": 1.4677129705888563e19 });
        let d = node_difficulty(&v);
        let rel = (d - 3417233412.77).abs() / 3417233412.77;
        assert!(rel < 1e-4, "from bits: {d}");
        let v = json!({ "difficulty_blake2b": 1.4677129705888563e19 });
        let w = node_difficulty(&v);
        let rel = (w - d).abs() / d;
        assert!(rel < 1e-4, "from work: {w} vs bits {d}");
    }

    #[test]
    fn bits_1d00ffff_is_difficulty_one_and_garbage_is_zero() {
        assert!((node_difficulty(&json!({ "bits": "1d00ffff" })) - 1.0).abs() < 1e-9);
        assert_eq!(node_difficulty(&json!({ "bits": "zz" })), 0.0);
        assert_eq!(node_difficulty(&json!({})), 0.0);
    }
}
