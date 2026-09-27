//! An in-process stand-in for the relay's public listener (§8.3), served on
//! `127.0.0.1:0` so the bots use their real HTTP port (`HttpRelay`) and the
//! herald's `/h/me` its real `/f/quota` call.
//!
//! The relay itself is Node (`permutation-gateway/src/frontier/`, tested by
//! its own suite); this stand-in keeps the parts of its contract the bots
//! and the chain can observe, so the system test exercises them end to end:
//!
//! - `GET /f/relay[?citizen=]`: a fee payer drawn uniformly from the relay
//!   pool, the chain's blockhash, the citizen's quota;
//! - `POST /f/relay`, `POST /f/join`: the **shape allowlist**
//!   (`[SetComputeUnitLimit(budget), SetComputeUnitPrice(0),
//!   SetLoadedAccountsDataSizeLimit(L(kind)), one Frontier player
//!   instruction]` or a settle shape without an authority signature), the
//!   program id, fee payer ∈ pool, every other signer's signature, a
//!   Reveal → `400 UseRevealRoute`, Join only on `/f/join`, a Depart's tip ∈
//!   the three presets (`TipNotPreset`), the per-citizen game-day quota
//!   (`429 QuotaExceeded`), the settle requester signature and its citizen
//!   (else the client-address bucket), then co-sign → **drain guard**
//!   (simulate with signatures; a failure is `{ok: false, code}` with the
//!   program's error name and nothing is sent or charged; the fee payer's
//!   Δlamports ≤ fee + the kind's allowance) → send;
//! - `POST /f/reveal`: forwarded to the keeper's loopback `/v1/reveal` with
//!   its bearer token, the answer passed through;
//! - `GET /f/quota?citizen=`: `{left, resetsAt, lamportsLeft}`.
//!
//! Not emulated (the gateway's own tests cover them): per-IP limits (bots
//! are loopback clients, exempt), invites (the in-process season has no
//! join gate), the replay cache, the operator routes and the v1.3 exact
//! account-key and writability check.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use fclient::abi::{size, tag};
use fclient::addr::Addresses;
use fclient::budgets::Budgets;
use fclient::decode::{Citizen, Season};
use fclient::ports::ChainPort;
use fclient::{Address, Keypair, Signer, Transaction};
use localnet::InProcess;
use serde_json::{json, Value};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Sponsored transactions per citizen per game day (D4, days 0–6).
pub const QUOTA_PER_DAY: u32 = 40;
/// The relay pool size (§8.3: 150 keys).
pub const POOL: u32 = 150;

/// Where `/f/reveal` goes.
#[derive(Clone, Debug)]
pub struct KeeperLink {
    pub addr: SocketAddr,
    pub token: String,
}

/// What the relay did, by route, instruction and result (for the report).
#[derive(Default, Debug, Clone)]
pub struct RelayStats {
    pub by: BTreeMap<(String, String, String), u64>,
    /// Signatures the relay sent (landed or not: the chain decides).
    pub sent: Vec<(u8, fclient::ports::Signature)>,
}

pub struct RelayState {
    pub ip: InProcess,
    pub program: Address,
    pub addrs: Addresses,
    pub pool: Vec<Keypair>,
    pub budgets: Budgets,
    pub keeper: Mutex<Option<KeeperLink>>,
    rng: Mutex<u64>,
    /// (bucket, game day) → sponsored transactions.
    quota: Mutex<BTreeMap<(String, i64), u32>>,
    pub stats: Mutex<RelayStats>,
}

fn refuse(status: StatusCode, code: &str, detail: impl Into<String>) -> Response {
    (
        status,
        Json(json!({"ok": false, "code": code, "error": detail.into()})),
    )
        .into_response()
}

impl RelayState {
    pub fn new(ip: InProcess, program: Address, season_id: u64, master: &[u8; 32]) -> RelayState {
        let pool = (0..POOL)
            .map(|i| fclient::payers::derive(master, fclient::payers::RELAY_POOL, i))
            .collect();
        RelayState {
            ip,
            program,
            addrs: Addresses::new(program, season_id),
            pool,
            budgets: bots_budgets(),
            keeper: Mutex::new(None),
            rng: Mutex::new(
                0x9E37_79B9_7F4A_7C15
                    ^ u64::from_le_bytes(master[..8].try_into().unwrap_or([0; 8])),
            ),
            quota: Mutex::new(BTreeMap::new()),
            stats: Mutex::new(RelayStats::default()),
        }
    }

    fn draw(&self) -> &Keypair {
        let mut s = self.rng.lock().unwrap_or_else(|p| p.into_inner());
        // xorshift64*: uniform enough for a fee-payer draw (no round-robin).
        *s ^= *s >> 12;
        *s ^= *s << 25;
        *s ^= *s >> 27;
        let x = s.wrapping_mul(0x2545_F491_4F6C_DD1D);
        &self.pool[(x % self.pool.len() as u64) as usize]
    }

    fn season(&self) -> Option<Season> {
        let a = self.ip.lock().account(&self.addrs.season)?;
        Season::decode(&a.data).ok()
    }

    fn day(&self) -> (i64, i64) {
        let now = self.ip.lock().unix_timestamp();
        let g = self.season().map_or(now, |s| s.genesis_ts);
        let day = (now - g).max(0) / 86_400;
        (day, g + (day + 1) * 86_400)
    }

    fn used(&self, bucket: &str) -> u32 {
        let (day, _) = self.day();
        *self
            .quota
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&(bucket.to_string(), day))
            .unwrap_or(&0)
    }

    fn charge(&self, bucket: &str) {
        let (day, _) = self.day();
        *self
            .quota
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry((bucket.to_string(), day))
            .or_default() += 1;
    }

    fn count(&self, route: &str, tg: Option<u8>, result: &str) {
        let name = tg
            .and_then(frontier_abi::tags::Ix::from_tag)
            .map(|i| format!("{i:?}"))
            .unwrap_or_else(|| "-".into());
        *self
            .stats
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .by
            .entry((route.to_string(), name, result.to_string()))
            .or_default() += 1;
    }

    /// The three tip presets (§8.3) from the Season's reveal parameters.
    pub fn tip_presets(&self) -> [u64; 3] {
        let Some(s) = self.season() else {
            return [0; 3];
        };
        let l = if s.reveal_loaded_limit > 0 {
            s.reveal_loaded_limit
        } else {
            self.budgets.get(tag::REVEAL).loaded_limit
        };
        let t = fclient::fees::min_tip_lamports(s.min_reveal_priority_milli, s.reveal_cu_limit, l);
        [t, (3 * t).div_ceil(2), 2 * t]
    }
}

fn bots_budgets() -> Budgets {
    let v: Value = serde_json::from_str(frontier_bots::txb::BUDGETS_JSON).expect("budgets.json");
    Budgets::from_json(&v).expect("budgets table")
}

type St = Arc<RelayState>;

async fn relay_info(State(st): State<St>, Query(q): Query<BTreeMap<String, String>>) -> Response {
    let (bh, lvbh) = st.ip.lock().latest_blockhash();
    let payer = st.draw().pubkey();
    let mut v = json!({
        "feePayer": payer.to_string(),
        "blockhash": bh.to_string(),
        "lastValidBlockHeight": lvbh,
        "programId": st.program.to_string(),
    });
    if let Some(c) = q.get("citizen") {
        let (_, resets) = st.day();
        v["quota"] = json!({
            "left": QUOTA_PER_DAY.saturating_sub(st.used(&format!("citizen:{c}"))),
            "resetsAt": resets,
        });
    }
    Json(v).into_response()
}

async fn quota(State(st): State<St>, Query(q): Query<BTreeMap<String, String>>) -> Response {
    let Some(c) = q.get("citizen") else {
        return refuse(StatusCode::BAD_REQUEST, "BadRequest", "citizen");
    };
    let (_, resets) = st.day();
    Json(json!({
        "left": QUOTA_PER_DAY.saturating_sub(st.used(&format!("citizen:{c}"))),
        "resetsAt": resets,
        "lamportsLeft": null,
    }))
    .into_response()
}

/// The player tags the relay sponsors and the settle tags it relays.
fn player_tag(t: u8) -> bool {
    matches!(t, 0x30..=0x33 | 0x40..=0x46 | 0x50)
}
fn settle_tag(t: u8) -> bool {
    t == tag::SETTLE_EXPLORE || t == tag::SETTLE_TRANSIT
}

/// The checks before co-signing. Returns the Frontier instruction's tag and
/// the quota bucket.
fn check(
    st: &RelayState,
    path: &str,
    b: &Value,
    t: &Transaction,
) -> Result<(u8, String), Box<Response>> {
    let bad = |c: &str, d: &str| Err(Box::new(refuse(StatusCode::BAD_REQUEST, c, d)));
    let msg = &t.message;
    if msg.instructions.len() != 4 {
        return bad("RelayRejected", "four instructions");
    }
    let payer = msg.account_keys[0];
    if !st.pool.iter().any(|k| k.pubkey() == payer) {
        return bad("RelayRejected", "fee payer not in the relay pool");
    }
    let cb = fclient::addr::compute_budget_program();
    for i in 0..3 {
        let p = msg.account_keys[msg.instructions[i].program_id_index as usize];
        if p != cb {
            return bad("RelayRejected", "compute budget prefix");
        }
    }
    let ix = &msg.instructions[3];
    if msg.account_keys[ix.program_id_index as usize] != st.program {
        return bad("RelayRejected", "program id");
    }
    let Some(&tg) = ix.data.first() else {
        return bad("RelayRejected", "empty instruction");
    };
    if tg == tag::REVEAL {
        return bad("UseRevealRoute", "reveals go to /f/reveal");
    }
    if !player_tag(tg) && !settle_tag(tg) {
        return bad("RelayRejected", "instruction not sponsored");
    }
    if (path == "/f/join") != (tg == tag::JOIN) {
        return bad("RelayRejected", "Join only on /f/join");
    }
    let pb = fclient::tx::parse_budget(msg);
    let want = st.budgets.get(tg);
    if pb.cu_price != Some(0) {
        return bad("RelayRejected", "SetComputeUnitPrice must be 0");
    }
    if pb.effective_cu_limit() != want.cu_limit || pb.effective_loaded_limit() != want.loaded_limit
    {
        return bad("RelayRejected", "budget or loaded limit");
    }
    // Every signer except the fee payer has signed.
    let data = t.message_data();
    let n = msg.header.num_required_signatures as usize;
    let mut signers = vec![];
    for i in 1..n {
        if !t.signatures[i].verify(msg.account_keys[i].as_ref(), &data) {
            return bad("BadSignature", "a signer's signature");
        }
        signers.push(msg.account_keys[i]);
    }
    let acct =
        |i: usize| -> Option<Address> { ix.accounts.get(i).map(|&k| msg.account_keys[k as usize]) };
    if settle_tag(tg) {
        if !signers.is_empty() {
            return bad("RelayRejected", "a settle shape has no authority signature");
        }
        // §8.3 v1.3: charged to the citizen only with the requester's
        // signature of the message by its wallet or unexpired session.
        let bucket = (|| {
            let req: Address = b.get("requester")?.as_str()?.parse().ok()?;
            let sig = B64.decode(b.get("requesterSig")?.as_str()?).ok()?;
            let sig: [u8; 64] = sig.try_into().ok()?;
            let citizen: Address = b.get("citizen")?.as_str()?.parse().ok()?;
            if !fclient::ports::Signature::from(sig).verify(req.as_ref(), &data) {
                return None;
            }
            let c = st.ip.lock().account(&citizen)?;
            let cz = Citizen::decode(&c.data).ok()?;
            if st.addrs.citizen(&cz.wallet) != citizen {
                return None;
            }
            let now = st.ip.lock().unix_timestamp();
            (req == cz.wallet || (req == cz.session && cz.session_expiry > now))
                .then(|| format!("citizen:{citizen}"))
        })()
        .unwrap_or_else(|| "client:127.0.0.1".to_string());
        return Ok((tg, bucket));
    }
    if signers.is_empty() {
        return bad("RelayRejected", "no authority signature");
    }
    if tg == tag::DEPART {
        let tip = ix
            .data
            .get(210..218)
            .map(|x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
            .unwrap_or(0);
        if !st.tip_presets().contains(&tip) {
            return bad("TipNotPreset", "tip not one of the three presets");
        }
    }
    let citizen = if tg == tag::JOIN { acct(4) } else { acct(3) };
    let Some(citizen) = citizen else {
        return bad("RelayRejected", "citizen account");
    };
    Ok((tg, format!("citizen:{citizen}")))
}

/// Lamports the fee payer may spend beyond the fee for `tg` (§8.3).
fn allowance(st: &RelayState, tg: u8, ix_data: &[u8]) -> u64 {
    let s = st.season();
    match tg {
        tag::JOIN => fclient::abi::rent(size::CITIZEN),
        tag::FILE_TICKET => fclient::abi::rent(size::HOLDING),
        tag::DEPART => {
            let tip = ix_data
                .get(210..218)
                .map(|x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
                .unwrap_or(0);
            tip + s.as_ref().map_or(0, |s| s.march_fee + s.seal_bond)
        }
        _ => 0,
    }
}

async fn sponsored(st: St, path: &'static str, b: Value) -> Response {
    let Some(wire) = b
        .get("tx")
        .and_then(|x| x.as_str())
        .and_then(|s| B64.decode(s).ok())
    else {
        st.count(path, None, "BadBody");
        return refuse(StatusCode::BAD_REQUEST, "BadBody", "tx");
    };
    let mut t = match fclient::tx::from_wire(&wire) {
        Ok(t) => t,
        Err(e) => {
            st.count(path, None, "InvalidTransaction");
            return refuse(StatusCode::BAD_REQUEST, "InvalidTransaction", e);
        }
    };
    let (tg, bucket) = match check(&st, path, &b, &t) {
        Ok(x) => x,
        Err(r) => {
            let tg = t
                .message
                .instructions
                .get(3)
                .and_then(|i| i.data.first().copied());
            st.count(path, tg, &format!("refused:{}", r.status().as_u16()));
            return *r;
        }
    };
    if st.used(&bucket) >= QUOTA_PER_DAY {
        st.count(path, Some(tg), "QuotaExceeded");
        let (_, resets) = st.day();
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"ok": false, "code": "QuotaExceeded", "retryAt": resets})),
        )
            .into_response();
    }
    // Co-sign as the fee payer.
    let payer_key = t.message.account_keys[0];
    let Some(kp) = st.pool.iter().find(|k| k.pubkey() == payer_key) else {
        return refuse(StatusCode::BAD_REQUEST, "RelayRejected", "fee payer");
    };
    let bh = t.message.recent_blockhash;
    if let Err(e) = t.try_partial_sign(&[kp], bh) {
        return refuse(StatusCode::BAD_REQUEST, "RelayRejected", e.to_string());
    }
    let signed = fclient::tx::wire(&t);
    // Drain guard: simulate with signatures.
    let sim = match st.ip.simulate(&signed).await {
        Ok(s) => s,
        Err(e) => {
            st.count(path, Some(tg), "SimulateRejected");
            return refuse(StatusCode::BAD_REQUEST, "RelayRejected", e.to_string());
        }
    };
    if let Some(err) = sim.err {
        let code = sim
            .code
            .and_then(fclient::abi::error_name)
            .map(String::from)
            .unwrap_or_else(|| "SimulationFailed".to_string());
        st.count(path, Some(tg), &code);
        return (
            StatusCode::CONFLICT,
            Json(json!({"ok": false, "code": code, "error": err})),
        )
            .into_response();
    }
    let pre = st.ip.lock().balance(&payer_key);
    let post = sim.post_balance(&payer_key).unwrap_or(pre);
    let fee = fclient::tx::fee_lamports(&t.message);
    let ix_data = t.message.instructions[3].data.clone();
    if pre.saturating_sub(post) > fee + allowance(&st, tg, &ix_data) {
        st.count(path, Some(tg), "DrainGuard");
        return refuse(
            StatusCode::BAD_REQUEST,
            "RelayRejected",
            format!("drain guard: Δ {} > fee {fee} + allowance", pre - post),
        );
    }
    match st.ip.send(&signed).await {
        Ok(sig) => {
            st.charge(&bucket);
            st.count(path, Some(tg), "sent");
            st.stats
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .sent
                .push((tg, sig));
            Json(json!({"ok": true, "signature": sig.to_string()})).into_response()
        }
        Err(e) => {
            st.count(path, Some(tg), "SendRejected");
            refuse(StatusCode::BAD_REQUEST, "RelayRejected", e.to_string())
        }
    }
}

async fn post_relay(State(st): State<St>, Json(b): Json<Value>) -> Response {
    sponsored(st, "/f/relay", b).await
}

async fn post_join(State(st): State<St>, Json(b): Json<Value>) -> Response {
    sponsored(st, "/f/join", b).await
}

/// `POST` with a bearer token over one loopback connection.
async fn post_bearer(
    addr: SocketAddr,
    path: &str,
    token: &str,
    body: &Value,
) -> Result<(u16, Value), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| e.to_string())?;
    let b = body.to_string();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
        b.len()
    );
    s.write_all(req.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    s.read_to_end(&mut out).await.map_err(|e| e.to_string())?;
    let r = fclient::http::parse_response(&out).map_err(|e| e.to_string())?;
    Ok((
        r.status,
        serde_json::from_slice(&r.body).unwrap_or(Value::Null),
    ))
}

async fn post_reveal(State(st): State<St>, Json(b): Json<Value>) -> Response {
    let link = st.keeper.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let Some(k) = link else {
        st.count("/f/reveal", Some(tag::REVEAL), "NoKeeper");
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "KeeperUnavailable",
            "no keeper",
        );
    };
    match post_bearer(k.addr, "/v1/reveal", &k.token, &b).await {
        Ok((status, v)) => {
            let code = v
                .get("code")
                .and_then(|c| c.as_str())
                .unwrap_or(if status < 300 { "accepted" } else { "refused" })
                .to_string();
            st.count("/f/reveal", Some(tag::REVEAL), &code);
            (
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                Json(v),
            )
                .into_response()
        }
        Err(e) => {
            st.count("/f/reveal", Some(tag::REVEAL), "KeeperUnavailable");
            refuse(StatusCode::BAD_GATEWAY, "KeeperUnavailable", e)
        }
    }
}

pub fn router(st: St) -> Router {
    Router::new()
        .route("/f/relay", get(relay_info).post(post_relay))
        .route("/f/join", post(post_join))
        .route("/f/reveal", post(post_reveal))
        .route("/f/quota", get(quota))
        .fallback(|| async { refuse(StatusCode::NOT_FOUND, "NotFound", "") })
        .with_state(st)
}

/// A running relay stand-in.
pub struct Running {
    pub addr: SocketAddr,
    pub state: St,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    pub fn stop(self) {
        self.task.abort();
    }
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Serves the stand-in on `127.0.0.1:0` and funds its pool (10 SOL each).
pub async fn serve(st: RelayState) -> Result<Running, String> {
    {
        let mut c = st.ip.lock();
        for k in &st.pool {
            c.airdrop(&k.pubkey(), 10_000_000_000)?;
        }
    }
    let st = Arc::new(st);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    let app = router(st.clone());
    let task = tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    Ok(Running {
        addr,
        state: st,
        task,
    })
}
