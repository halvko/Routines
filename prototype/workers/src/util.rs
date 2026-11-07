//! Small helpers shared by the Worker and the Durable Objects.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use worker::*;

pub fn now_ms() -> u64 {
    Date::now().as_millis()
}

/// 32 random bytes, base64url.
pub fn random_secret() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("getrandom");
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn sha256_b64(input: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(input.as_bytes()))
}

/// Tokens are `<user>.<secret>`, so the Worker can route them to the user's
/// Durable Object without a lookup. Only the hash is stored.
pub fn new_token(user: &str) -> String {
    format!("{user}.{}", random_secret())
}

pub fn token_user(token: &str) -> Option<&str> {
    let (user, _) = token.split_once('.')?;
    valid_user(user).then_some(user)
}

/// User names are lowercase letters, digits and dashes, so they are safe in
/// cookies, tokens and Durable Object names.
pub fn valid_user(user: &str) -> bool {
    (1..=32).contains(&user.len())
        && user.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn json_response(value: &serde_json::Value, status: u16) -> Result<Response> {
    Ok(Response::from_json(value)?.with_status(status))
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub const SESSION_COOKIE: &str = "__Host-session";

pub fn session_cookie(req: &Request) -> Option<String> {
    let header = req.headers().get("cookie").ok()??;
    header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == SESSION_COOKIE).then(|| value.to_string())
    })
}

/// Builds an internal request to a Durable Object: JSON body, plus the
/// caller's origin (needed for WebAuthn and OAuth URLs).
pub async fn call_do(
    env: &Env,
    binding: &str,
    name: &str,
    path: &str,
    origin: &str,
    body: &serde_json::Value,
    extra: &[(&str, &str)],
) -> Result<Response> {
    let stub = env.durable_object(binding)?.get_by_name(name)?;
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    headers.set("x-origin", origin)?;
    for (k, v) in extra {
        headers.set(k, v)?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.to_string().into()));
    let req = Request::new_with_init(&format!("https://do{path}"), &init)?;
    stub.fetch_with_request(req).await
}

/// Responses from a Durable Object have immutable headers; copy one into a
/// fresh JSON response so headers can be added.
pub async fn copy_json(mut resp: Response, status: u16, extra: &[(&str, &str)]) -> Result<Response> {
    let body = resp.text().await?;
    let mut out = Response::ok(body)?.with_status(status);
    out.headers_mut().set("content-type", "application/json")?;
    for (k, v) in extra {
        out.headers_mut().set(k, v)?;
    }
    Ok(out)
}
