//! The hand-rolled OAuth authorization server subset Claude's custom
//! connectors need (see docs/research/2026-11-rust-server-hosting.md):
//! RFC 9728 and RFC 8414 metadata, DCR, CIMD, authorize with PKCE S256,
//! and token with refresh-token rotation.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use worker::*;

use crate::util::*;

const CLAUDE_CALLBACKS: [&str; 2] = [
    "https://claude.ai/api/mcp/auth_callback",
    "https://claude.com/api/mcp/auth_callback",
];

pub fn resource(origin: &str) -> String {
    format!("{origin}/mcp")
}

pub fn resource_metadata_url(origin: &str) -> String {
    format!("{origin}/.well-known/oauth-protected-resource/mcp")
}

pub async fn handle(req: &mut Request, env: &Env, origin: &str, path: &str) -> Result<Option<Response>> {
    let method = req.method();
    Ok(Some(match (method, path) {
        (Method::Get, "/.well-known/oauth-protected-resource" | "/.well-known/oauth-protected-resource/mcp") => {
            Response::from_json(&json!({
                "resource": resource(origin),
                "authorization_servers": [origin],
                "scopes_supported": ["routines", "offline_access"],
                "bearer_methods_supported": ["header"],
            }))?
        }
        (Method::Get, "/.well-known/oauth-authorization-server") => Response::from_json(&json!({
            "issuer": origin,
            "authorization_endpoint": format!("{origin}/authorize"),
            "token_endpoint": format!("{origin}/token"),
            "registration_endpoint": format!("{origin}/register"),
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none"],
            "scopes_supported": ["routines", "offline_access"],
            "client_id_metadata_document_supported": true,
            "authorization_response_iss_parameter_supported": true,
        }))?,
        (Method::Post, "/register") => register(req, env, origin).await?,
        (Method::Get, "/authorize") => {
            let query = req.url()?.query().unwrap_or("").to_string();
            authorize(req, env, origin, &query, false).await?
        }
        (Method::Post, "/authorize") => {
            let form = req.text().await?;
            authorize(req, env, origin, &form, true).await?
        }
        (Method::Post, "/token") => token(req, env, origin).await?,
        _ => return Ok(None),
    }))
}

fn is_loopback(uri: &Url) -> bool {
    uri.scheme() == "http" && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// Redirect URIs match exactly, except that loopback ports may differ
/// (RFC 8252 section 7.3).
fn redirect_matches(registered: &Value, uri: &str) -> bool {
    let Ok(asked) = Url::parse(uri) else { return false };
    registered.as_array().into_iter().flatten().filter_map(|r| r.as_str()).any(|r| {
        r == uri
            || Url::parse(r).is_ok_and(|r| {
                is_loopback(&r) && is_loopback(&asked) && r.host_str() == asked.host_str() && r.path() == asked.path()
            })
    })
}

async fn register(req: &mut Request, env: &Env, origin: &str) -> Result<Response> {
    let Ok(mut body) = req.json::<Value>().await else {
        return json_response(&json!({ "error": "invalid_client_metadata" }), 400);
    };
    let uris = body["redirect_uris"].as_array().cloned().unwrap_or_default();
    let ok = !uris.is_empty()
        && uris.iter().all(|u| {
            u.as_str()
                .is_some_and(|u| CLAUDE_CALLBACKS.contains(&u) || Url::parse(u).is_ok_and(|u| is_loopback(&u)))
        });
    if !ok {
        return json_response(&json!({ "error": "invalid_redirect_uri" }), 400);
    }
    body["token_endpoint_auth_method"] = json!("none");
    body["grant_types"] = json!(["authorization_code", "refresh_token"]);
    body["response_types"] = json!(["code"]);
    let resp = call_do(env, "DIRECTORY", "directory", "/register", origin, &body, &[]).await?;
    copy_json(resp, 201, &[("cache-control", "no-store")]).await
}

/// Looks a client up: a DCR client in the directory, or, when the id is an
/// HTTPS URL, a Client ID Metadata Document fetched from it. The fetch runs
/// under `global_fetch_strictly_public` (wrangler.toml), so it cannot reach
/// private addresses.
async fn client(env: &Env, origin: &str, client_id: &str) -> Result<Option<Value>> {
    if client_id.starts_with("https://") {
        let Ok(url) = Url::parse(client_id) else { return Ok(None) };
        let mut resp = match Fetch::Url(url).send().await {
            Ok(r) if r.status_code() == 200 => r,
            _ => return Ok(None),
        };
        let doc: Value = match resp.json().await {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };
        return Ok((doc["client_id"] == client_id && doc["redirect_uris"].is_array()).then_some(doc));
    }
    let mut resp =
        call_do(env, "DIRECTORY", "directory", "/client", origin, &json!({ "client_id": client_id }), &[]).await?;
    if resp.status_code() != 200 {
        return Ok(None);
    }
    Ok(serde_json::from_str(&resp.text().await?).ok())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AuthParams {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    state: Option<String>,
    scope: String,
    resource: Option<String>,
}

async fn authorize(req: &Request, env: &Env, origin: &str, params: &str, approve: bool) -> Result<Response> {
    let Ok(p) = serde_urlencoded::from_str::<AuthParams>(params) else {
        return page_error("Malformed authorization request.");
    };
    let Some(client) = client(env, origin, &p.client_id).await? else {
        return page_error("Unknown client.");
    };
    if !redirect_matches(&client["redirect_uris"], &p.redirect_uri) {
        return page_error("This redirect URI is not registered for the client.");
    }
    // From here on, errors go back to the client.
    let back = |error: &str| -> Result<Response> {
        let mut u = Url::parse(&p.redirect_uri)?;
        u.query_pairs_mut().append_pair("error", error).append_pair("iss", origin);
        if let Some(s) = &p.state {
            u.query_pairs_mut().append_pair("state", s);
        }
        Response::redirect(u)
    };
    if p.response_type != "code" {
        return back("unsupported_response_type");
    }
    if p.code_challenge_method != "S256" || p.code_challenge.len() != 43 {
        return back("invalid_request");
    }
    let expected = resource(origin);
    if p.resource.as_deref().is_some_and(|r| r != expected) {
        return back("invalid_target");
    }

    let session = session_cookie(req).unwrap_or_default();
    let signed_in = match token_user(&session) {
        Some(user) => {
            let mut r = call_do(env, "USER", user, "/session/check", origin, &json!({ "session": session }), &[]).await?;
            r.json::<Value>().await?["ok"] == true
        }
        None => false,
    };
    if !signed_in {
        if approve {
            return page_error("Your session expired. Go back and try again.");
        }
        let mut next = Url::parse(&format!("{origin}/"))?;
        next.query_pairs_mut().append_pair("next", &format!("/authorize?{params}"));
        return Response::redirect(next);
    }

    if !approve {
        let name = client["client_name"].as_str().unwrap_or(&p.client_id);
        let hidden: String = serde_urlencoded::from_str::<Vec<(String, String)>>(params)
            .unwrap_or_default()
            .iter()
            .map(|(k, v)| format!(r#"<input type="hidden" name="{}" value="{}">"#, esc(k), esc(v)))
            .collect();
        return Response::from_html(format!(
            r#"<!doctype html><meta name="viewport" content="width=device-width">
<title>Allow access</title><body style="font-family:system-ui;max-width:28rem;margin:2rem auto;padding:0 1rem">
<h1>Allow {name} to read your routines?</h1>
<form method="post" action="/authorize">{hidden}<button id="allow" type="submit">Allow</button></form>"#,
            name = esc(name)
        ));
    }

    let user = token_user(&session).unwrap_or_default();
    let body = json!({
        "session": session, "client_id": p.client_id, "redirect_uri": p.redirect_uri,
        "code_challenge": p.code_challenge, "resource": expected, "scope": p.scope,
    });
    let mut r = call_do(env, "USER", user, "/oauth/code", origin, &body, &[]).await?;
    let v: Value = r.json().await?;
    let Some(code) = v["code"].as_str() else {
        return back("server_error");
    };
    let mut u = Url::parse(&p.redirect_uri)?;
    u.query_pairs_mut().append_pair("code", code).append_pair("iss", origin);
    if let Some(s) = &p.state {
        u.query_pairs_mut().append_pair("state", s);
    }
    Response::redirect(u)
}

async fn token(req: &mut Request, env: &Env, origin: &str) -> Result<Response> {
    let form: HashMap<String, String> = serde_urlencoded::from_str(&req.text().await?).unwrap_or_default();
    let presented = match form.get("grant_type").map(String::as_str) {
        Some("authorization_code") => form.get("code"),
        Some("refresh_token") => form.get("refresh_token"),
        _ => return json_response(&json!({ "error": "unsupported_grant_type" }), 400),
    };
    let Some(user) = presented.and_then(|t| token_user(t)) else {
        return json_response(&json!({ "error": "invalid_grant" }), 400);
    };
    let body = serde_json::to_value(&form)?;
    let resp = call_do(env, "USER", user, "/oauth/token", origin, &body, &[]).await?;
    let status = resp.status_code();
    copy_json(resp, status, &[("cache-control", "no-store")]).await
}

fn page_error(msg: &str) -> Result<Response> {
    Ok(Response::from_html(format!(
        "<!doctype html><title>Error</title><p>{}</p>",
        esc(msg)
    ))?
    .with_status(400))
}
