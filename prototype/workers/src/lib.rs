//! THROWAWAY prototype for issue #3: does Rust on Cloudflare Workers carry
//! decision 0005? This is not the real server; see README.md.

use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version};
use serde_json::{Value, json};
use worker::*;

mod directory;
mod mcp;
mod oauth;
mod page;
mod user;
mod util;

pub use directory::Directory;
pub use user::UserObject;
use util::*;

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let origin = url.origin().ascii_serialization();
    let path = url.path().to_string();

    if let Some(resp) = oauth::handle(&mut req, &env, &origin, &path).await? {
        return Ok(resp);
    }
    match (req.method(), path.as_str()) {
        (Method::Get, "/") => Response::from_html(page::INDEX),
        (Method::Get, "/hash") => hash(),
        (Method::Get, p) if p.starts_with("/ping/") => {
            let name = &p["/ping/".len()..];
            if !valid_user(name) {
                return Response::error("bad name", 400);
            }
            call_do(&env, "USER", name, "/ping", &origin, &Value::Null, &[]).await
        }
        (Method::Post, p) if p.starts_with("/api/passkey/") => {
            passkey(&mut req, &env, &origin, p).await
        }
        (Method::Get, "/api/routines") => {
            with_session(&req, &env, &origin, "/routines/list", json!({})).await
        }
        (Method::Post, "/api/routines") => {
            let body: Value = req.json().await?;
            with_session(&req, &env, &origin, "/routines/add", body).await
        }
        (Method::Get, "/api/ws") => websocket(&req, &env).await,
        (Method::Post, "/mcp") => mcp_endpoint(&mut req, &env, &origin).await,
        (_, "/mcp") => Ok(Response::error("method not allowed", 405)?
            .with_headers(Headers::from_iter([("allow", "POST")]))),
        _ => Response::error("not found", 404),
    }
}

/// One Argon2id hash at OWASP's baseline (decision 0004), to measure CPU.
fn hash() -> Result<Response> {
    let params = Params::new(19456, 2, 1, None).map_err(|e| Error::RustError(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let phc = argon
        .hash_password(b"correct horse battery staple")
        .map_err(|e| Error::RustError(e.to_string()))?
        .to_string();
    Response::from_json(&json!({ "phc": phc }))
}

/// `/api/passkey/{register,login}/{start,finish}` with `{"user": ...}`.
/// The finish steps return a session, which becomes the cookie.
async fn passkey(req: &mut Request, env: &Env, origin: &str, path: &str) -> Result<Response> {
    let body: Value = req.json().await?;
    let user = body["user"].as_str().unwrap_or("").to_string();
    if !valid_user(&user) {
        return json_response(
            &json!({ "error": "user names are 1-32 of a-z, 0-9 and -" }),
            400,
        );
    }
    let session = session_cookie(req).unwrap_or_default();
    let do_path = &path["/api".len()..];
    let extra = [("x-session", session.as_str())];
    let mut resp = call_do(env, "USER", &user, do_path, origin, &body, &extra).await?;
    if !path.ends_with("/finish") || resp.status_code() != 200 {
        return Ok(resp);
    }
    let v: Value = resp.json().await?;
    let session = v["session"].as_str().unwrap_or("");
    let mut out = Response::from_json(&json!({ "ok": true }))?;
    out.headers_mut().set(
        "set-cookie",
        &format!(
            "{SESSION_COOKIE}={session}; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000"
        ),
    )?;
    Ok(out)
}

async fn with_session(
    req: &Request,
    env: &Env,
    origin: &str,
    do_path: &str,
    body: Value,
) -> Result<Response> {
    let session = session_cookie(req).unwrap_or_default();
    let Some(user) = token_user(&session) else {
        return Response::error("not signed in", 401);
    };
    let extra = [("x-session", session.as_str())];
    call_do(env, "USER", user, do_path, origin, &body, &extra).await
}

async fn websocket(req: &Request, env: &Env) -> Result<Response> {
    if req.headers().get("upgrade")?.as_deref() != Some("websocket") {
        return Response::error("expected a WebSocket upgrade", 426);
    }
    let session = session_cookie(req).unwrap_or_default();
    let Some(user) = token_user(&session) else {
        return Response::error("not signed in", 401);
    };
    let headers = Headers::new();
    headers.set("upgrade", "websocket")?;
    headers.set("x-session", &session)?;
    let mut init = RequestInit::new();
    init.with_headers(headers);
    let stub = env.durable_object("USER")?.get_by_name(user)?;
    stub.fetch_with_request(Request::new_with_init("https://do/ws", &init)?)
        .await
}

/// The MCP endpoint: a bearer token from `/token`, routed by its prefix to
/// the user's Durable Object, which checks it and answers.
async fn mcp_endpoint(req: &mut Request, env: &Env, origin: &str) -> Result<Response> {
    let unauthorized = || -> Result<Response> {
        let mut r = Response::error("unauthorized", 401)?;
        r.headers_mut().set(
            "www-authenticate",
            &format!(
                r#"Bearer resource_metadata="{}""#,
                oauth::resource_metadata_url(origin)
            ),
        )?;
        Ok(r)
    };
    let auth = req.headers().get("authorization")?.unwrap_or_default();
    let Some(token) = auth.strip_prefix("Bearer ") else {
        return unauthorized();
    };
    let Some(user) = token_user(token) else {
        return unauthorized();
    };
    let Ok(body) = req.json::<Value>().await else {
        return json_response(
            &json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } }),
            400,
        );
    };
    let version = req
        .headers()
        .get("mcp-protocol-version")?
        .unwrap_or_default();
    let mut extra = vec![("x-token", token)];
    if !version.is_empty() {
        extra.push(("x-mcp-protocol-version", version.as_str()));
    }
    let resp = call_do(env, "USER", user, "/mcp", origin, &body, &extra).await?;
    if resp.status_code() == 401 {
        return unauthorized();
    }
    Ok(resp)
}
