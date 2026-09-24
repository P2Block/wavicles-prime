//! Node poller: tracks the tip and difficulty, sizes the TIDES window, relays new-block
//! notifications, and confirms found blocks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::state::{now, Shared, Tip};

/// 2^32: hashes per diff-1 share (share difficulty 1 is target 2^224).
const HASHES_PER_DIFF1: f64 = 4_294_967_296.0;

/// Network difficulty in diff-1 share units -- the unit TIDES work is counted in.
///
/// SHA256d Bitcoin nodes report `difficulty` already relative to the diff-1 target. BTCB2
/// (Bitcoin BLAKE2b) omits `difficulty` and reports `difficulty_blake2b`, which is
/// GetBlockProof(bits) = 2^256 / (target + 1): expected *hashes* per block, 2^32 times the
/// share-unit value. Reading only `difficulty` left the TIDES target at 0 on BTCB2 (window
/// never trimmed); using `difficulty_blake2b` unconverted would saturate the u64 target.
pub(crate) fn share_difficulty(info: &Value) -> f64 {
    if let Some(d) = info.get("difficulty").and_then(Value::as_f64) {
        return d;
    }
    info.get("difficulty_blake2b").and_then(Value::as_f64).map_or(0.0, |d| d / HASHES_PER_DIFF1)
}

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
                let difficulty = share_difficulty(&info);
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

#[cfg(test)]
mod tests {
    use super::share_difficulty;
    use serde_json::json;

    #[test]
    fn sha256d_difficulty_is_used_as_is() {
        assert_eq!(share_difficulty(&json!({ "difficulty": 1234.5 })), 1234.5);
    }

    #[test]
    fn btcb2_difficulty_is_converted_from_hashes_to_share_units() {
        // getblockchaininfo at BTCB2 height 973,858 (bits 1900edba)
        let d = share_difficulty(&json!({ "difficulty_blake2b": 1.986469847209289e19 }));
        assert!((d - 1.986469847209289e19 / 4_294_967_296.0).abs() < 1.0);
        // matches the gateway's own job difficulty for the same bits
        assert!((d - 4_625_110_531.0).abs() / d < 1e-3);
        // 8x window target fits a u64 (the unconverted value would not)
        assert!(d * 8.0 < u64::MAX as f64);
        assert!(1.986469847209289e19 * 8.0 > u64::MAX as f64);
    }

    #[test]
    fn missing_difficulty_is_zero() {
        assert_eq!(share_difficulty(&json!({ "blocks": 1 })), 0.0);
    }
}
