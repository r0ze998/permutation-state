//! The operator's part of the start order (offchain design §11.4 step 3,
//! M1 contract §5.7, I-09, I-51, I-54): AnnounceSeason by the program's
//! upgrade authority, the 24-h lead at the pre-season scale (2,000: ≈ 43 s
//! of 400-ms slots), the target scale, CreateSeason, InitBeaconLogs and
//! the 48 JoinShards. ConsumeGenesisSeed is keeper A's (its `beacon` role).

use std::time::Duration;

use fclient::addr::Addresses;
use fclient::{ix, Keypair, Signer};
use frontier_abi::presets::SeasonParams;
use serde_json::{json, Value};

use crate::chain::Chain;
use crate::config::Beacon;

/// AnnounceSeason's creation bond (test SOL; I-09 default 1 SOL).
pub const BOND: u64 = 1_000_000_000;
/// The lead after the announcement (§5.7: 24 h) plus a minute.
pub const LEAD_SECS: i64 = 86_400 + 60;

/// The season parameters of a stack run: the M1 local preset with the
/// drand key the program accepts (the test key's hash for a test-beacon
/// build, quicknet's for the release build).
pub fn params(beacon: Beacon) -> SeasonParams {
    let mut p = frontier_abi::presets::M1_LOCAL_7D;
    if beacon == Beacon::TestKey {
        p.quicknet_pk_hash = fclient::beacon::pk_hash(&fclient::beacon::TestKey::new().pk96);
    }
    p
}

/// Runs the operator steps; returns what the report needs.
pub async fn run(
    chain: &Chain,
    addrs: &Addresses,
    authority: &Keypair,
    beacon: Beacon,
    preseason_scale: f64,
    scale: f64,
    log: &dyn Fn(&str, Value),
) -> Result<Value, String> {
    let land = Duration::from_secs(60);
    let p = params(beacon);
    let sp = p.to_bytes();
    let payout = permutation_rules::frontier::payout::PayoutParams::REV3.to_borsh();
    let ph = frontier_abi::presets::params_hash(&sp, &payout);
    // Announce at the run's scale (at 2,000 a slot is 800 game seconds, so
    // the lead would shrink below the program's minimum before landing),
    // with 20 slots of margin, then run the lead at the pre-season scale.
    chain.set_scale(scale).await?;
    let s0 = chain.status().await?.slot;
    while chain.status().await?.slot < s0 + 2 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let st = chain.status().await?;
    let t_create_min = st.now + LEAD_SECS + (20.0 * 0.4 * scale).ceil() as i64;
    let auth = authority.pubkey();
    chain
        .send_op(
            &[ix::announce_season(addrs, auth, ph, t_create_min, BOND)],
            &[authority],
            land,
        )
        .await?;
    let announced = chain.status().await?;
    chain.set_scale(preseason_scale).await?;
    log(
        "announced",
        json!({"slot": announced.slot, "game": announced.now, "t_create_min": t_create_min}),
    );
    let wall0 = std::time::Instant::now();
    // The pre-season: one slot at the pre-season scale moves 0.4 × 2,000 =
    // 800 game seconds, so the 24-h lead is ≈ 108 slots ≈ 43 s.
    loop {
        let s = chain.status().await?;
        if s.now >= t_create_min {
            break;
        }
        let left = (t_create_min - s.now) as f64 / (0.4 * preseason_scale);
        if wall0.elapsed() > Duration::from_secs(3_600) {
            return Err("the pre-season did not reach t_create_min in an hour of wall time".into());
        }
        tokio::time::sleep(Duration::from_millis(
            (left * 400.0).clamp(100.0, 1_000.0) as u64
        ))
        .await;
    }
    let preseason_wall = wall0.elapsed().as_secs_f64();
    chain.set_scale(scale).await?;
    // The new rate applies from the next slot boundary.
    let s0 = chain.status().await?.slot;
    while chain.status().await?.slot < s0 + 2 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    chain
        .send_op(
            &[ix::create_season(addrs, auth, &sp, &payout)],
            &[authority],
            land,
        )
        .await?;
    chain
        .send_op(&[ix::init_beacon_logs(addrs, auth)], &[authority], land)
        .await?;
    for f in 0..fclient::abi::FACTIONS {
        chain
            .send_op(&[ix::init_shards(addrs, auth, f)], &[authority], land)
            .await?;
    }
    let s = chain
        .season(addrs)
        .await?
        .ok_or("the season account is absent after CreateSeason")?;
    if s.status != fclient::abi::status::CREATED {
        return Err(format!("season status {} after CreateSeason", s.status));
    }
    let created = chain.status().await?;
    let v = json!({
        "announce_slot": announced.slot, "announce_game": announced.now, "t_create_min": t_create_min,
        "preseason_scale": preseason_scale, "preseason_wall_secs": preseason_wall,
        "created_slot": created.slot, "created_game": created.now,
        "genesis_ts": s.genesis_ts, "genesis_round": s.genesis_round, "end_bell": s.end_bell,
        "join_close_bell": s.join_close_bell, "bell_secs": s.bell_secs,
        "quicknet_pk_hash": hex::encode(s.quicknet_pk_hash), "params_hash": hex::encode(ph),
    });
    log("created", v.clone());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_key_build_gets_the_test_key_hash() {
        let t = params(Beacon::TestKey);
        let a = params(Beacon::Archive);
        assert_eq!(
            a.quicknet_pk_hash,
            frontier_abi::presets::M1_LOCAL_7D.quicknet_pk_hash
        );
        assert_ne!(t.quicknet_pk_hash, a.quicknet_pk_hash);
        let qn = fclient::beacon::quicknet_info();
        assert_eq!(
            fclient::beacon::pk_hash(&qn.public_key),
            a.quicknet_pk_hash,
            "the release preset pins quicknet"
        );
    }
}
