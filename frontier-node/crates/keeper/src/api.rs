//! The keeper's loopback API (M1 contract §8.2; skeleton in W2-F).
//!
//! Bound to a loopback address only; every route needs `Authorization:
//! Bearer <token>` (the token file the relay shares, like the gateway's
//! operator token).
//!
//! | route | W2-F |
//! |---|---|
//! | `GET /v1/status` | duties, pools, spend, latencies (live) |
//! | `GET /metrics` | Prometheus text (live) |
//! | `POST /v1/reveal` | shape checks (`400`), plaintext rules (`422 BadPlaintext`), then (W3-C) the accept path against chain ([`crate::reveal_accept`]: `409 CommitMismatch` / `TransitState`, `410 WindowClosed`, `422 BadPlaintext` against the transit), then queued with a track id (`202`; the same material again answers the same track). The sending is W4-C's pipeline |
//! | `POST /v1/nudge` | queued (`{queued, blocking: []}`); the blocking set is W4-C's |
//! | `GET /v1/track/{id}` | the track's state |

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use serde_json::{json, Value};

use crate::reveal_accept::{RevealGate, RevealReq};
use crate::Shared;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Clone)]
struct ApiState {
    shared: Arc<Mutex<Shared>>,
    token: Arc<String>,
    /// The accept path against chain (W3-C); `None` queues after the shape
    /// checks only (unit tests of the routes).
    gate: Option<Arc<dyn RevealGate>>,
}

fn lock(s: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    s.lock().unwrap_or_else(|p| p.into_inner())
}

fn authorized(st: &ApiState, h: &HeaderMap) -> bool {
    let want = format!("Bearer {}", st.token);
    h.get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            // constant-time over the header length
            v.len() == want.len()
                && v.bytes()
                    .zip(want.bytes())
                    .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                    == 0
        })
}

fn err(code: StatusCode, name: &str, detail: &str) -> Response {
    (code, Json(json!({"code": name, "detail": detail}))).into_response()
}

async fn status(State(st): State<ApiState>, h: HeaderMap) -> Response {
    if !authorized(&st, &h) {
        return err(StatusCode::UNAUTHORIZED, "Unauthorized", "bearer token");
    }
    Json(lock(&st.shared).status.clone()).into_response()
}

async fn metrics(State(st): State<ApiState>, h: HeaderMap) -> Response {
    if !authorized(&st, &h) {
        return err(StatusCode::UNAUTHORIZED, "Unauthorized", "bearer token");
    }
    (
        [("content-type", "text/plain; version=0.0.4")],
        lock(&st.shared).metrics.clone(),
    )
        .into_response()
}

fn b64_len(v: &Value, k: &str, n: usize) -> Result<Vec<u8>, String> {
    let s = v
        .get(k)
        .and_then(|x| x.as_str())
        .ok_or(format!("missing `{k}`"))?;
    let b = B64.decode(s).map_err(|_| format!("`{k}` is not base64"))?;
    if b.len() != n {
        return Err(format!("`{k}` must be {n} bytes, got {}", b.len()));
    }
    Ok(b)
}

/// Checks the owner reveal material of `POST /v1/reveal` (§8.2).
pub fn check_reveal(v: &Value) -> Result<(), (StatusCode, &'static str, String)> {
    let bad = |d: String| (StatusCode::BAD_REQUEST, "BadRequest", d);
    v.get("holding")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<solana_address::Address>().ok())
        .ok_or_else(|| bad("`holding` must be a base58 address".into()))?;
    let slot = v
        .get("transit_slot")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| bad("missing `transit_slot`".into()))?;
    if slot >= fclient::abi::TRANSIT_SLOTS as u64 {
        return Err(bad("`transit_slot` must be 0-3".into()));
    }
    let plain = b64_len(v, "plain_b64", fclient::seal::PLAIN_LEN).map_err(bad)?;
    b64_len(v, "salt_b64", 32).map_err(bad)?;
    b64_len(v, "ct_hash_b64", 32).map_err(bad)?;
    let p = fclient::seal::unpack(&plain.try_into().expect("37"));
    // The rules of I-28 that need no chain read (host and arrive are the
    // plaintext's own here; W3-C checks them against the transit record).
    fclient::seal::validate(&p, p.host_id, p.arrive_bell).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "BadPlaintext",
            format!("{e:?}"),
        )
    })
}

/// The request's material as the accept path takes it (shape already checked).
fn reveal_req(v: &Value) -> Option<RevealReq> {
    let b = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .and_then(|s| B64.decode(s).ok())
    };
    Some(RevealReq {
        holding: v.get("holding")?.as_str()?.parse().ok()?,
        transit_slot: v.get("transit_slot")?.as_u64()? as u8,
        plain: b("plain_b64")?.try_into().ok()?,
        salt: b("salt_b64")?.try_into().ok()?,
        ct_hash: b("ct_hash_b64")?.try_into().ok()?,
    })
}

async fn reveal(State(st): State<ApiState>, h: HeaderMap, Json(v): Json<Value>) -> Response {
    if !authorized(&st, &h) {
        return err(StatusCode::UNAUTHORIZED, "Unauthorized", "bearer token");
    }
    if let Err((c, name, d)) = check_reveal(&v) {
        return err(c, name, &d);
    }
    let Some(req) = reveal_req(&v) else {
        return err(StatusCode::BAD_REQUEST, "BadRequest", "reveal material");
    };
    let mut item = v.clone();
    if let Some(g) = &st.gate {
        match g.check(&req).await {
            Ok(a) => {
                item["checked"] = json!(true);
                item["host_id"] = json!(a.host_id.to_string());
                item["faction"] = json!(a.faction);
                item["arrive"] = json!(a.arrive);
                item["dest"] = json!([a.dest.0, a.dest.1]);
                item["tile"] = json!(a.tile);
                item["region"] = json!(a.region);
                item["commit_hex"] = json!(hex::encode(a.commit));
                item["close"] = json!(a.close);
            }
            Err(r) => {
                let code = StatusCode::from_u16(r.http).unwrap_or(StatusCode::CONFLICT);
                return err(code, r.code, &r.detail);
            }
        }
    } else {
        item["checked"] = json!(false);
    }
    let dedupe = format!(
        "{}:{}:{}",
        req.holding,
        req.transit_slot,
        hex::encode(fclient::seal::commit(&req.plain, &req.salt))
    );
    let mut s = lock(&st.shared);
    if let Some(id) = s.reveal_keys.get(&dedupe).cloned() {
        return (
            StatusCode::ACCEPTED,
            Json(json!({"accepted": true, "track": id})),
        )
            .into_response();
    }
    s.next_track += 1;
    let id = format!("r{}", s.next_track);
    s.tracks.insert(id.clone(), json!({"state": "queued"}));
    s.reveal_keys.insert(dedupe, id.clone());
    s.reveals.push((id.clone(), item));
    (
        StatusCode::ACCEPTED,
        Json(json!({"accepted": true, "track": id})),
    )
        .into_response()
}

async fn nudge(State(st): State<ApiState>, h: HeaderMap, Json(v): Json<Value>) -> Response {
    if !authorized(&st, &h) {
        return err(StatusCode::UNAUTHORIZED, "Unauthorized", "bearer token");
    }
    let ok = v
        .get("province")
        .and_then(|p| p.as_array())
        .is_some_and(|a| a.len() == 2 && a.iter().all(|x| x.as_i64().is_some()))
        && v.get("bell").and_then(|b| b.as_u64()).is_some();
    if !ok {
        return err(
            StatusCode::BAD_REQUEST,
            "BadRequest",
            "`{province: [P, Q], bell}`",
        );
    }
    lock(&st.shared).nudges.push(v);
    Json(json!({"queued": true, "blocking": []})).into_response()
}

async fn track(State(st): State<ApiState>, h: HeaderMap, Path(id): Path<String>) -> Response {
    if !authorized(&st, &h) {
        return err(StatusCode::UNAUTHORIZED, "Unauthorized", "bearer token");
    }
    match lock(&st.shared).tracks.get(&id) {
        Some(t) => Json(t.clone()).into_response(),
        None => err(StatusCode::NOT_FOUND, "NotFound", &id),
    }
}

pub fn router(shared: Arc<Mutex<Shared>>, token: String) -> Router {
    router_with(shared, token, None)
}

/// The routes with the accept path of `/v1/reveal` behind `gate`.
pub fn router_with(
    shared: Arc<Mutex<Shared>>,
    token: String,
    gate: Option<Arc<dyn RevealGate>>,
) -> Router {
    let st = ApiState {
        shared,
        token: Arc::new(token),
        gate,
    };
    Router::new()
        .route("/v1/status", get(status))
        .route("/metrics", get(metrics))
        .route("/v1/reveal", post(reveal))
        .route("/v1/nudge", post(nudge))
        .route("/v1/track/{id}", get(track))
        .with_state(st)
}

/// A running API server.
pub struct Running {
    pub addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    pub fn stop(self) {
        self.task.abort();
    }
}

/// Serves on `addr`, which must be a loopback address (port 0 for tests).
pub async fn serve(
    addr: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    token: String,
) -> Result<Running, String> {
    serve_with(addr, shared, token, None).await
}

/// [`serve`] with the `/v1/reveal` accept path against chain.
pub async fn serve_with(
    addr: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    token: String,
    gate: Option<Arc<dyn RevealGate>>,
) -> Result<Running, String> {
    if !addr.ip().is_loopback() {
        return Err(format!("the keeper API binds loopback only, not {addr}"));
    }
    if token.len() < 16 {
        return Err("the API token must be at least 16 characters".into());
    }
    let l = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    let app = router_with(shared, token, gate);
    let task = tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    Ok(Running { addr, task })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn call(
        addr: SocketAddr,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let b = body.map(|v| v.to_string()).unwrap_or_default();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: x\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        );
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = vec![];
        s.read_to_end(&mut out).await.unwrap();
        let text = String::from_utf8_lossy(&out).to_string();
        let code: u16 = text[9..12].parse().unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (code, serde_json::from_str(body).unwrap_or(json!(body)))
    }

    #[tokio::test]
    async fn routes_need_the_token_and_check_shapes() {
        assert!(serve(
            "0.0.0.0:0".parse().unwrap(),
            Default::default(),
            "x".repeat(32)
        )
        .await
        .is_err());
        let shared: Arc<Mutex<Shared>> = Default::default();
        lock(&shared).status = json!({"slot": 5});
        lock(&shared).metrics = "frontier_keeper_slot 5\n".into();
        let tok = "t".repeat(32);
        let r = serve("127.0.0.1:0".parse().unwrap(), shared.clone(), tok.clone())
            .await
            .unwrap();
        let a = r.addr;
        assert_eq!(call(a, "GET", "/v1/status", None, None).await.0, 401);
        assert_eq!(
            call(a, "GET", "/v1/status", Some("wrong"), None).await.0,
            401
        );
        let (c, v) = call(a, "GET", "/v1/status", Some(&tok), None).await;
        assert_eq!((c, v), (200, json!({"slot": 5})));
        let (c, v) = call(a, "GET", "/metrics", Some(&tok), None).await;
        assert_eq!(c, 200);
        assert!(v.as_str().unwrap().contains("frontier_keeper_slot 5"));
        // /v1/reveal
        let p = fclient::seal::Plain {
            version: 1,
            host_id: 42,
            arrive_bell: 9,
            stance: 1,
            ..Default::default()
        };
        let good = json!({
            "holding": solana_address::Address::new_from_array([3; 32]).to_string(),
            "transit_slot": 1,
            "plain_b64": B64.encode(fclient::seal::pack(&p)),
            "salt_b64": B64.encode([1u8; 32]),
            "ct_hash_b64": B64.encode([2u8; 32]),
        });
        let (c, v) = call(a, "POST", "/v1/reveal", Some(&tok), Some(good.clone())).await;
        assert_eq!(c, 202);
        let id = v["track"].as_str().unwrap().to_string();
        let (c, v) = call(a, "GET", &format!("/v1/track/{id}"), Some(&tok), None).await;
        assert_eq!((c, v["state"].as_str()), (200, Some("queued")));
        // The same material again: the same track, queued once.
        let (c, v) = call(a, "POST", "/v1/reveal", Some(&tok), Some(good.clone())).await;
        assert_eq!((c, v["track"].as_str()), (202, Some(id.as_str())));
        let mut short = good.clone();
        short["salt_b64"] = json!(B64.encode([1u8; 31]));
        assert_eq!(
            call(a, "POST", "/v1/reveal", Some(&tok), Some(short))
                .await
                .0,
            400
        );
        let mut bad = good.clone();
        let q = fclient::seal::Plain { stance: 9, ..p };
        bad["plain_b64"] = json!(B64.encode(fclient::seal::pack(&q)));
        let (c, v) = call(a, "POST", "/v1/reveal", Some(&tok), Some(bad)).await;
        assert_eq!((c, v["code"].as_str()), (422, Some("BadPlaintext")));
        // /v1/nudge
        let (c, v) = call(
            a,
            "POST",
            "/v1/nudge",
            Some(&tok),
            Some(json!({"province": [2, -1], "bell": 77})),
        )
        .await;
        assert_eq!((c, v["queued"].as_bool()), (200, Some(true)));
        assert_eq!(lock(&shared).nudges.len(), 1);
        assert_eq!(lock(&shared).reveals.len(), 1);
        assert_eq!(
            call(a, "GET", "/v1/track/nope", Some(&tok), None).await.0,
            404
        );
        r.stop();
    }
}
