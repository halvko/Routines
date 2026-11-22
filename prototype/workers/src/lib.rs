//! Throwaway prototype for decision 0005: can the server run as Rust on
//! Cloudflare Workers, with one SQLite-backed Durable Object per user?
//!
//! What it proves (see `README.md` for how each was checked):
//! - the per-user sync log of 0003: idempotent push, pull after N, and a
//!   WebSocket "poke" to the user's other devices, all in the user's object;
//! - an MCP endpoint served by the same Rust Worker, whose writes go through
//!   that same log, so a change made by Claude reaches every device like a
//!   change made on a phone;
//! - Argon2id at OWASP's minimum parameters inside the runtime.
//!
//! Auth is a stub (`Authorization: Bearer dev-<user>`); the real sign-in
//! (0004) and the OAuth flow Claude needs are later pieces.

use worker::*;

mod bench;
mod mcp;
mod passkey;
mod user;

pub use user::User;

/// The user a request acts for. Stub: a bearer token `dev-<user>`, or the
/// `/u/<user>/` path segment for the sync routes.
fn user_from_bearer(req: &Request) -> Option<String> {
    let header = req.headers().get("Authorization").ok().flatten()?;
    let user = header.strip_prefix("Bearer dev-")?;
    valid_user(user).then(|| user.to_string())
}

fn valid_user(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 64
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The stub for the user's Durable Object. One object per user, addressed by
/// name, so every device and Claude reach the same single writer.
pub(crate) fn user_stub(env: &Env, user: &str) -> Result<Stub> {
    env.durable_object("USER")?.get_by_name(user)
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    let url = req.url()?;
    let path = url.path().to_string();
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();

    match (req.method(), segments.as_slice()) {
        (Method::Get, [""]) => Response::ok("routines workers prototype"),
        (Method::Get, ["bench", "argon2"]) => bench::argon2(),
        (Method::Get, ["bench", "noop"]) => Response::ok("ok"),
        (Method::Get, ["passkey.html"]) => Response::from_html(include_str!("passkey.html")),

        // Sync API (0003). Forwarded unchanged to the user's object.
        (_, ["u", user, rest @ ..]) if valid_user(user) => {
            let inner = format!("https://user/{}{}", rest.join("/"), query(&url));
            let mut init = RequestInit::new();
            init.with_method(req.method()).with_headers(req.headers().clone());
            if let Some(body) = req.inner().body() {
                init.with_body(Some(body.into()));
            }
            user_stub(&env, user)?
                .fetch_with_request(Request::new_with_init(&inner, &init)?)
                .await
        }

        // MCP (Streamable HTTP, JSON responses only).
        (Method::Post, ["mcp"]) => {
            let Some(user) = user_from_bearer(&req) else {
                return Response::error("missing or bad bearer token", 401);
            };
            mcp::handle(req, &env, &user).await
        }
        (Method::Get, ["mcp"]) => {
            // No server-initiated stream in this prototype.
            Response::error("method not allowed", 405)
        }

        _ => Response::error("not found", 404),
    }
}

fn query(url: &Url) -> String {
    url.query().map(|q| format!("?{q}")).unwrap_or_default()
}
