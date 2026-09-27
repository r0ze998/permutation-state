//! The herald's HTTP origin (contract §8.4): `/h/*` (files and live
//! answers), `WS /h/ws`, `/frontier/*` (the static web client) and `/gw/*`
//! (the relay's public listener behind the same origin).
//!
//! **Caching.** Per-bell files (`province/…/{b}`, `clash/…`, `overview/…/
//! {b}.bin`) are `immutable`; `latest` answers `max-age=2` (province) and
//! `max-age=5` (overview); a bell-region file is `immutable` once final
//! (archived and resolved everywhere), `max-age=5` before; `/h/season` is
//! `max-age=30` with an ETag; `/h/me` is `no-store`; a full `/h/events`
//! page (500) is `immutable`. Precompressed `.gz` siblings are sent to
//! clients that accept gzip (`Vary: Accept-Encoding`).
//!
//! **Security headers** (as `permutation-server/src/play/http.rs`) on
//! every answer: `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`,
//! `X-Content-Type-Options: nosniff`, and a CSP — the web design §12 policy
//! for `/frontier/*`, `default-src 'none'; frame-ancestors 'none'`
//! elsewhere. `/h/*` answers carry `Access-Control-Allow-Origin: *` (public
//! data; the page's loopback `?herald=` override reads them cross-origin).
//!
//! **`/gw/*`** is the relay's public listener (§8.3) on this origin, as
//! `permutation-server/src/play/proxy.rs`: GET/POST/OPTIONS only, a safe
//! path and query, a body ≤ 64 KiB; of the browser's headers only
//! `Content-Type` goes on; `X-Forwarded-For` is set to the browser's
//! address (the peer, or — when the peer is this machine, a reverse proxy
//! in front — the last address in its `X-Forwarded-For`), which the relay
//! trusts only from this loopback peer; only the relay's status, content
//! type and body come back.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::Router;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_address::Address;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

use crate::files::{gz_path, Out};
use crate::fold::{Diff, Fold};
use crate::records::{record_json, B64};
use crate::views::{self, SeasonStatic};
use crate::ws::{self, WsCfg, WsStats};
use base64::Engine;

/// The Frontier pages' policy (web design §12).
pub const WEB_CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; frame-ancestors 'none'";
/// Every other answer's policy.
pub const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'";
pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Events per `/h/events` page.
pub const EVENTS_PAGE: usize = 500;
/// Largest `/gw/*` request body.
pub const GW_MAX_BODY: usize = 64 * 1024;
const GW_MAX_ANSWER: u64 = 16 << 20;
const GW_TIMEOUT: Duration = Duration::from_secs(90);
const GW_CONNECT: Duration = Duration::from_secs(5);

/// Everything a request handler reads.
pub struct App {
    pub fold: Arc<RwLock<Fold>>,
    pub out: Out,
    pub web: Option<PathBuf>,
    /// The relay's public listener, `host:port`.
    pub relay: Option<String>,
    pub season: SeasonStatic,
    pub diffs: broadcast::Sender<Arc<Diff>>,
    /// The findex index (for `/h/events`).
    pub index: Option<PathBuf>,
    pub events: Mutex<Option<findex::Reader>>,
    pub ws: WsCfg,
    pub ws_stats: Arc<WsStats>,
    cache: Mutex<LiveCache>,
}

/// `(fold version, live clock)`: what a cached season answer depends on.
type SeasonKey = (u64, Option<(u64, i64)>);
/// A season answer: body and ETag.
type SeasonAnswer = Arc<(Vec<u8>, String)>;

#[derive(Default)]
struct LiveCache {
    season: Option<(SeasonKey, SeasonAnswer)>,
    overview: std::collections::BTreeMap<u16, (u64, Arc<Vec<u8>>)>,
}

impl App {
    pub fn new(
        fold: Arc<RwLock<Fold>>,
        out: Out,
        season: SeasonStatic,
        diffs: broadcast::Sender<Arc<Diff>>,
    ) -> App {
        App {
            fold,
            out,
            web: None,
            relay: None,
            season,
            diffs,
            index: None,
            events: Mutex::new(None),
            ws: WsCfg::default(),
            ws_stats: Arc::new(WsStats::default()),
            cache: Mutex::new(LiveCache::default()),
        }
    }
}

pub type Shared = Arc<App>;

/// The router (callers serve it with `into_make_service_with_connect_info`).
pub fn router(app: Shared) -> Router {
    Router::new()
        .route("/h/season", get(season))
        .route("/h/status", get(status))
        .route("/h/overview/{ring}/{file}", get(overview))
        .route("/h/province/{pq}/{bell}", get(province))
        .route("/h/clash/{pq}/{bell}", get(clash))
        .route("/h/bell/{bell}/region/{r}", get(bell_region))
        .route("/h/me/{wallet}", get(me))
        .route("/h/events", get(events))
        .route("/h/ws", get(ws_route))
        .route("/", get(|| async { redirect("/frontier/") }))
        .route("/frontier", get(|| async { redirect("/frontier/") }))
        .route("/frontier/", get(web))
        .route("/frontier/{*path}", get(web))
        .route("/gw", any(gw))
        .route("/gw/{*path}", any(gw))
        .fallback(|| async { err(StatusCode::NOT_FOUND, "NotFound") })
        .layer(middleware::from_fn(security_headers))
        .with_state(app)
}

/// Serves `router(app)` on `listener` until the future is dropped.
pub async fn serve(listener: tokio::net::TcpListener, app: Shared) -> std::io::Result<()> {
    axum::serve(
        listener,
        router(app).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

async fn security_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    let csp = if path == "/frontier" || path.starts_with("/frontier/") {
        WEB_CSP
    } else {
        API_CSP
    };
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(csp),
    );
    if path.starts_with("/h/") {
        h.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
    }
    res
}

fn err(status: StatusCode, code: &str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json!({"ok": false, "code": code}).to_string(),
    )
        .into_response()
}

fn redirect(to: &'static str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, to)]).into_response()
}

fn json_answer(v: &Value, cache: &str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        serde_json::to_vec(v).unwrap_or_default(),
    )
        .into_response()
}

fn accepts_gzip(h: &HeaderMap) -> bool {
    h.get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|e| e.trim().starts_with("gzip")))
}

/// A file of the output directory (or `None` if absent).
fn file_answer(app: &App, rel: &str, ctype: &str, cache: &str, h: &HeaderMap) -> Option<Response> {
    let p = app.out.path(rel)?;
    if !p.is_file() {
        return None;
    }
    let gz = gz_path(&p);
    let (body, enc) = if accepts_gzip(h) && gz.is_file() {
        (std::fs::read(&gz).ok()?, Some("gzip"))
    } else {
        (std::fs::read(&p).ok()?, None)
    };
    let mut r = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CACHE_CONTROL, cache)
        .header(header::VARY, "Accept-Encoding");
    if let Some(e) = enc {
        r = r.header(header::CONTENT_ENCODING, e);
    }
    r.body(Body::from(body)).ok()
}

fn parse_pq(s: &str) -> Option<(i16, i16)> {
    let (p, q) = s.split_once(',')?;
    Some((p.parse().ok()?, q.parse().ok()?))
}

fn read_fold(app: &App) -> std::sync::RwLockReadGuard<'_, Fold> {
    app.fold.read().unwrap_or_else(|e| e.into_inner())
}

async fn season(State(app): State<Shared>, h: HeaderMap) -> Response {
    let entry = {
        let f = read_fold(&app);
        let mut c = app.cache.lock().unwrap_or_else(|e| e.into_inner());
        match &c.season {
            Some((v, e)) if *v == (f.version, f.live) => e.clone(),
            _ => {
                let Some(v) = views::season_json(&f, &app.season) else {
                    return err(StatusCode::SERVICE_UNAVAILABLE, "NoSeasonYet");
                };
                let body = serde_json::to_vec(&v).unwrap_or_default();
                let etag = format!("\"{}\"", &hex::encode(Sha256::digest(&body))[..32]);
                let e = Arc::new((body, etag));
                c.season = Some(((f.version, f.live), e.clone()));
                e
            }
        }
    };
    let (body, etag) = (&entry.0, &entry.1);
    let cache = "public, max-age=30";
    if h.get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag))
    {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.clone()),
                (header::CACHE_CONTROL, cache.into()),
            ],
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::CACHE_CONTROL, cache.into()),
            (header::ETAG, etag.clone()),
        ],
        body.clone(),
    )
        .into_response()
}

async fn status(State(app): State<Shared>) -> Response {
    let f = read_fold(&app);
    let a = f.st.alarms;
    json_answer(
        &json!({
            "v": 1, "foldedThrough": f.st.folded_through, "events": f.st.events.to_string(),
            "lastSlot": f.st.last_slot, "live": f.live.map(|l| json!([l.0, l.1])),
            "provinces": f.provinces.len(), "rings": f.rings.keys().collect::<Vec<_>>(),
            "alarms": {"rewrites": a.rewrites, "badRecords": a.bad_records, "clashMismatch": a.clash_mismatch,
                "clashUnchecked": a.clash_unchecked, "writeErrors": a.write_errors},
            "ws": {"open": app.ws_stats.open.load(std::sync::atomic::Ordering::SeqCst),
                "total": app.ws_stats.total.load(std::sync::atomic::Ordering::SeqCst),
                "droppedSlow": app.ws_stats.dropped_slow.load(std::sync::atomic::Ordering::SeqCst)},
        }),
        "no-store",
    )
}

async fn overview(
    State(app): State<Shared>,
    Path((ring, file)): Path<(u16, String)>,
    h: HeaderMap,
) -> Response {
    let bin = |b: Vec<u8>, cache: &str| {
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (header::CACHE_CONTROL, cache.to_string()),
            ],
            b,
        )
            .into_response()
    };
    if file == "latest.bin" {
        let f = read_fold(&app);
        let mut c = app.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((v, b)) = c.overview.get(&ring) {
            if *v == f.version {
                return bin(b.to_vec(), "public, max-age=5");
            }
        }
        return match f.overview_latest(ring) {
            Some(b) => {
                c.overview.insert(ring, (f.version, Arc::new(b.clone())));
                bin(b, "public, max-age=5")
            }
            None => err(StatusCode::NOT_FOUND, "NoSuchRing"),
        };
    }
    let Some(b) = file
        .strip_suffix(".bin")
        .and_then(|b| b.parse::<u32>().ok())
    else {
        return err(StatusCode::NOT_FOUND, "NotFound");
    };
    file_answer(
        &app,
        &format!("h/overview/{ring}/{b}.bin"),
        "application/octet-stream",
        IMMUTABLE,
        &h,
    )
    .unwrap_or_else(|| err(StatusCode::NOT_FOUND, "NotYet"))
}

async fn province(
    State(app): State<Shared>,
    Path((pq, bell)): Path<(String, String)>,
    h: HeaderMap,
) -> Response {
    let Some((p, q)) = parse_pq(&pq) else {
        return err(StatusCode::BAD_REQUEST, "BadProvince");
    };
    if bell == "latest" {
        let f = read_fold(&app);
        return match views::province_latest(&f, p, q) {
            Some(v) => json_answer(&v, "public, max-age=2"),
            None => err(StatusCode::NOT_FOUND, "NoSuchProvince"),
        };
    }
    let Ok(b) = bell.parse::<u32>() else {
        return err(StatusCode::BAD_REQUEST, "BadBell");
    };
    file_answer(
        &app,
        &format!("h/province/{p},{q}/{b}.json"),
        "application/json",
        IMMUTABLE,
        &h,
    )
    .unwrap_or_else(|| err(StatusCode::NOT_FOUND, "NotYet"))
}

async fn clash(
    State(app): State<Shared>,
    Path((pq, bell)): Path<(String, u32)>,
    h: HeaderMap,
) -> Response {
    let Some((p, q)) = parse_pq(&pq) else {
        return err(StatusCode::BAD_REQUEST, "BadProvince");
    };
    file_answer(
        &app,
        &format!("h/clash/{p},{q}/{bell}.json"),
        "application/json",
        IMMUTABLE,
        &h,
    )
    .unwrap_or_else(|| err(StatusCode::NOT_FOUND, "NotYet"))
}

async fn bell_region(
    State(app): State<Shared>,
    Path((bell, r)): Path<(u32, u8)>,
    h: HeaderMap,
) -> Response {
    let fin = read_fold(&app).bell_region_final(bell, r);
    file_answer(
        &app,
        &format!("h/bell/{bell}/region/{r}.json"),
        "application/json",
        if fin { IMMUTABLE } else { "public, max-age=5" },
        &h,
    )
    .unwrap_or_else(|| err(StatusCode::NOT_FOUND, "NotYet"))
}

async fn me(State(app): State<Shared>, Path(wallet): Path<String>) -> Response {
    let Ok(w) = wallet.parse::<Address>() else {
        return err(StatusCode::BAD_REQUEST, "BadWallet");
    };
    let citizen = {
        let f = read_fold(&app);
        Address::new_from_array(f.ctx.citizen(&w.to_bytes()))
    };
    let quota = match &app.relay {
        Some(r) => relay_quota(r, &citizen).await,
        None => Value::Null,
    };
    let f = read_fold(&app);
    json_answer(&views::me_json(&f, &w, quota), "no-store")
}

/// The relay's `/f/quota?citizen=` answer (`null` if it does not answer
/// within 2 s).
async fn relay_quota(relay: &str, citizen: &Address) -> Value {
    let url = format!("http://{relay}/f/quota?citizen={citizen}");
    match tokio::time::timeout(Duration::from_secs(2), fclient::http::get(&url)).await {
        Ok(Ok(r)) if (200..300).contains(&r.status) => {
            serde_json::from_slice(&r.body).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

async fn events(
    State(app): State<Shared>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let after: u64 = match q.get("after").map(|s| s.parse()) {
        None => 0,
        Some(Ok(a)) => a,
        Some(Err(_)) => return err(StatusCode::BAD_REQUEST, "BadAfter"),
    };
    let rows = {
        let mut g = app.events.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() {
            if let Some(p) = &app.index {
                *g = findex::Reader::open(p).ok();
            }
        }
        match g.as_ref() {
            Some(r) => r.events_after(after, EVENTS_PAGE),
            None => return err(StatusCode::SERVICE_UNAVAILABLE, "NoIndex"),
        }
    };
    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            eprintln!("herald: events: {e}");
            *app.events.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return err(StatusCode::SERVICE_UNAVAILABLE, "IndexBusy");
        }
    };
    let next = rows.last().map_or(after, |r| r.ev);
    let full = rows.len() == EVENTS_PAGE;
    let ev: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({"seq": r.ev.to_string(), "slot": r.slot, "sig": r.sig, "kind": r.kind, "bell": r.bell,
                "tx": r.seq, "body_b64": B64.encode(&r.body), "decoded": record_json(&r.body)})
        })
        .collect();
    json_answer(
        &json!({"v": 1, "events": ev, "next": next.to_string(), "full": full}),
        if full { IMMUTABLE } else { "public, max-age=2" },
    )
}

async fn ws_route(State(app): State<Shared>, req: Request) -> Response {
    let h = req.headers();
    let upgrade = h
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let conn = h
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
        });
    let version = h
        .get(header::SEC_WEBSOCKET_VERSION)
        .and_then(|v| v.to_str().ok())
        == Some("13");
    let Some(key) = h
        .get(header::SEC_WEBSOCKET_KEY)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
    else {
        return err(StatusCode::BAD_REQUEST, "NotAWebSocket");
    };
    if !(upgrade && conn && version) {
        return err(StatusCode::BAD_REQUEST, "NotAWebSocket");
    }
    if app.ws_stats.open.load(std::sync::atomic::Ordering::SeqCst) >= app.ws.max_sockets {
        return err(StatusCode::SERVICE_UNAVAILABLE, "TooManySockets");
    }
    let rx = app.diffs.subscribe();
    let on = hyper::upgrade::on(req);
    let cfg = app.ws;
    let stats = app.ws_stats.clone();
    tokio::spawn(async move {
        if let Ok(u) = on.await {
            ws::serve(hyper_util::rt::TokioIo::new(u), rx, cfg, stats).await;
        }
    });
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, ws::accept_key(&key))
        .body(Body::empty())
        .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Upgrade"))
}

// ------------------------------------------------------------------ static

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" | "sha256" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A name with a content hash (`app.1a2b3c4d.mjs`): long max-age.
fn hashed(name: &str) -> bool {
    name.split('.')
        .any(|s| s.len() >= 8 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

async fn web(State(app): State<Shared>, uri: Uri, h: HeaderMap) -> Response {
    let Some(root) = &app.web else {
        return err(StatusCode::NOT_FOUND, "NoWebClient");
    };
    let mut rel = uri
        .path()
        .strip_prefix("/frontier/")
        .unwrap_or("")
        .to_string();
    if rel.is_empty() || rel.ends_with('/') {
        rel.push_str("index.html");
    }
    let safe = rel
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
        && rel.split('/').all(|s| !s.is_empty() && !s.starts_with('.'));
    if !safe {
        return err(StatusCode::NOT_FOUND, "NotFound");
    }
    let p = root.join(&rel);
    let Ok(body) = std::fs::read(&p) else {
        return err(StatusCode::NOT_FOUND, "NotFound");
    };
    let name = rel.rsplit('/').next().unwrap_or(&rel);
    let etag = format!("\"{}\"", &hex::encode(Sha256::digest(&body))[..32]);
    let cache = if hashed(name) { IMMUTABLE } else { "no-cache" };
    if h.get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag))
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag), (header::CACHE_CONTROL, cache.into())],
        )
            .into_response();
    }
    let gz = gz_path(&p);
    let (body, enc) = match (accepts_gzip(&h), std::fs::read(&gz)) {
        (true, Ok(z)) => (z, Some("gzip")),
        _ => (body, None),
    };
    let mut r = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(name))
        .header(header::CACHE_CONTROL, cache)
        .header(header::ETAG, etag)
        .header(header::VARY, "Accept-Encoding");
    if let Some(e) = enc {
        r = r.header(header::CONTENT_ENCODING, e);
    }
    r.body(Body::from(body))
        .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Body"))
}

// ------------------------------------------------------------------ /gw

/// The browser's address: the peer, or — when the peer is this machine (a
/// reverse proxy or tunnel in front of the herald) — the last address in
/// the `X-Forwarded-For` it set.
pub fn client_ip(peer: IpAddr, h: &HeaderMap) -> IpAddr {
    let fwd = h
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|a| a.trim().parse().ok());
    match fwd {
        Some(ip) if peer.is_loopback() => ip,
        _ => peer,
    }
}

fn plain(v: &str) -> bool {
    v.bytes().all(|b| b == b'\t' || (0x20..0x7f).contains(&b))
}

async fn gw(
    State(app): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(relay) = &app.relay else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "NoRelay");
    };
    if !matches!(method, Method::GET | Method::POST | Method::OPTIONS) {
        return err(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed");
    }
    let path = match uri.path().strip_prefix("/gw") {
        Some("") => "/",
        Some(p) if p.starts_with('/') => p,
        _ => return err(StatusCode::NOT_FOUND, "NotAGatewayPath"),
    };
    let query = uri.query().unwrap_or("");
    let safe_path = path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/-_.~%".contains(&b))
        && !path.split('/').any(|s| s == "..");
    if !safe_path || !plain(query) || query.contains(' ') {
        return err(StatusCode::BAD_REQUEST, "BadGatewayPath");
    }
    if body.len() > GW_MAX_BODY {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "BodyTooLarge");
    }
    let body: &[u8] = if method == Method::POST { &body } else { &[] };
    let mut head = format!(
        "{method} {path}{}{query} HTTP/1.1\r\nHost: {relay}\r\nConnection: close\r\nContent-Length: {}\r\n",
        if query.is_empty() { "" } else { "?" },
        body.len()
    );
    if let Some(ct) = h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        if plain(ct) && ct.len() <= 256 {
            head.push_str(&format!("Content-Type: {ct}\r\n"));
        }
    }
    head.push_str(&format!(
        "X-Forwarded-For: {}\r\n\r\n",
        client_ip(peer.ip(), &h)
    ));
    let answer = async {
        let mut s = tokio::time::timeout(GW_CONNECT, tokio::net::TcpStream::connect(relay))
            .await
            .map_err(|_| "connect timeout".to_string())?
            .map_err(|e| e.to_string())?;
        s.write_all(head.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        s.write_all(body).await.map_err(|e| e.to_string())?;
        let mut buf = vec![];
        (&mut s)
            .take(GW_MAX_ANSWER)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        fclient::http::parse_response(&buf).map_err(|e| format!("{e:?}"))
    };
    let r = match tokio::time::timeout(GW_TIMEOUT, answer).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            eprintln!("herald: /gw{path}: {e}");
            return err(StatusCode::BAD_GATEWAY, "RelayUnavailable");
        }
        Err(_) => return err(StatusCode::GATEWAY_TIMEOUT, "RelayTimeout"),
    };
    let status = StatusCode::from_u16(r.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut b = Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(ct) = r.header("content-type") {
        if plain(ct) {
            b = b.header(header::CONTENT_TYPE, ct);
        }
    }
    b.body(Body::from(r.body))
        .unwrap_or_else(|_| err(StatusCode::BAD_GATEWAY, "RelayAnswer"))
}
