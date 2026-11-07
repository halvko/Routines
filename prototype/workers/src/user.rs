//! One Durable Object per user: their data (SQLite), passkeys, sessions,
//! OAuth codes and tokens, and their devices' WebSockets.

use std::cell::Cell;

use serde::Deserialize;
use serde_json::{Value, json};
use webauthn_rs::prelude::*;
use worker::*;

use crate::mcp;
use crate::util::*;

const SESSION_TTL_MS: u64 = 30 * 24 * 3600 * 1000;
const CODE_TTL_MS: u64 = 60 * 1000;
const ACCESS_TTL_MS: u64 = 3600 * 1000;
const REFRESH_TTL_MS: u64 = 30 * 24 * 3600 * 1000;

#[durable_object]
pub struct UserObject {
    state: State,
    /// When this instance was constructed, to tell cold wakes from warm ones.
    born: u64,
    requests: Cell<u64>,
}

#[derive(Deserialize)]
struct Row<T> {
    v: T,
}

impl DurableObject for UserObject {
    fn new(state: State, _env: Env) -> Self {
        let sql = state.storage().sql();
        for stmt in [
            "CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS passkeys (id INTEGER PRIMARY KEY, v TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS ceremonies (kind TEXT PRIMARY KEY, v TEXT NOT NULL, expires INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS sessions (hash TEXT PRIMARY KEY, expires INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS routines (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, created INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS oauth_codes (hash TEXT PRIMARY KEY, v TEXT NOT NULL, expires INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS oauth_tokens (hash TEXT PRIMARY KEY, kind TEXT NOT NULL, family TEXT NOT NULL, client_id TEXT NOT NULL, resource TEXT NOT NULL, scope TEXT NOT NULL, expires INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0)",
        ] {
            sql.exec(stmt, None).expect("schema");
        }
        Self {
            state,
            born: now_ms(),
            requests: Cell::new(0),
        }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        self.requests.set(self.requests.get() + 1);
        let path = req.path();
        let origin = req.headers().get("x-origin")?.unwrap_or_default();
        // The Worker passes the caller's session cookie, if any, in a header.
        let session = req.headers().get("x-session")?.unwrap_or_default();
        if path == "/ws" {
            return self.websocket(&session);
        }
        let body: Value = req.json().await.unwrap_or(Value::Null);
        let result = match path.as_str() {
            "/ping" => {
                Ok(json!({ "born": self.born, "now": now_ms(), "requests": self.requests.get() }))
            }
            "/passkey/register/start" => self.register_start(&origin, &session),
            "/passkey/register/finish" => self.register_finish(&origin, &body),
            "/passkey/login/start" => self.login_start(&origin),
            "/passkey/login/finish" => self.login_finish(&origin, &body),
            "/session/check" => self.check_session(&session).map(|ok| json!({ "ok": ok })),
            "/routines/list" => self.authed(&session, |s| s.list_routines()),
            "/routines/add" => self.authed(&session, |s| s.add_routine(&body)),
            "/oauth/code" => self.authed(&session, |s| s.issue_code(&body)),
            "/oauth/token/code" => self.redeem_code(&body),
            "/oauth/token/refresh" => self.refresh(&body),
            "/mcp" => return self.mcp(&origin, &req, &body),
            _ => return Response::error("not found", 404),
        };
        match result {
            Ok(v) => json_response(&v, 200),
            Err(Fail(status, msg)) => json_response(&json!({ "error": msg }), status),
        }
    }

    async fn websocket_message(&self, ws: WebSocket, msg: WebSocketIncomingMessage) -> Result<()> {
        if let WebSocketIncomingMessage::String(s) = msg {
            if s == "ping" {
                ws.send_with_str(json!({ "type": "pong", "born": self.born }).to_string())?;
            }
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        reason: String,
        _clean: bool,
    ) -> Result<()> {
        // The runtime reports 1005 (no status) or 1006 (abnormal), which close() refuses.
        let code = if code == 1005 || code == 1006 {
            1000
        } else {
            code as u16
        };
        let _ = ws.close(Some(code), Some(reason));
        Ok(())
    }
}

/// An error with an HTTP status, turned into `{"error": ...}`.
pub struct Fail(pub u16, pub String);

fn fail<T>(status: u16, msg: impl Into<String>) -> std::result::Result<T, Fail> {
    Err(Fail(status, msg.into()))
}

impl From<Error> for Fail {
    fn from(e: Error) -> Self {
        Fail(500, e.to_string())
    }
}

type Res<T> = std::result::Result<T, Fail>;

impl UserObject {
    fn sql(&self) -> SqlStorage {
        self.state.storage().sql()
    }

    fn user(&self) -> String {
        self.state.id().name().unwrap_or_default()
    }

    fn webauthn(&self, origin: &str) -> Res<Webauthn> {
        let url = Url::parse(origin).map_err(|_| Fail(400, "bad origin".into()))?;
        let rp_id = url.host_str().unwrap_or_default().to_string();
        WebauthnBuilder::new(&rp_id, &url)
            .and_then(|b| b.rp_name("Routines prototype").build())
            .map_err(|e| Fail(500, format!("webauthn config: {e}")))
    }

    fn user_uuid(&self) -> Res<Uuid> {
        let rows: Vec<Row<String>> = self
            .sql()
            .exec("SELECT v FROM meta WHERE k = 'uuid'", None)?
            .to_array()?;
        if let Some(r) = rows.first() {
            return Uuid::parse_str(&r.v).map_err(|e| Fail(500, e.to_string()));
        }
        let id = Uuid::new_v4();
        self.sql().exec(
            "INSERT INTO meta (k, v) VALUES ('uuid', ?)",
            vec![id.to_string().into()],
        )?;
        Ok(id)
    }

    fn passkeys(&self) -> Res<Vec<(i64, Passkey)>> {
        #[derive(Deserialize)]
        struct P {
            id: i64,
            v: String,
        }
        let rows: Vec<P> = self
            .sql()
            .exec("SELECT id, v FROM passkeys", None)?
            .to_array()?;
        rows.into_iter()
            .map(|r| {
                Ok((
                    r.id,
                    serde_json::from_str(&r.v).map_err(|e| Fail(500, e.to_string()))?,
                ))
            })
            .collect()
    }

    fn put_ceremony<T: serde::Serialize>(&self, kind: &str, state: &T) -> Res<()> {
        let v = serde_json::to_string(state).map_err(|e| Fail(500, e.to_string()))?;
        self.sql().exec(
            "INSERT OR REPLACE INTO ceremonies (kind, v, expires) VALUES (?, ?, ?)",
            vec![kind.into(), v.into(), ((now_ms() + 300_000) as i64).into()],
        )?;
        Ok(())
    }

    fn take_ceremony<T: serde::de::DeserializeOwned>(&self, kind: &str) -> Res<T> {
        let rows: Vec<Row<String>> = self
            .sql()
            .exec(
                "SELECT v FROM ceremonies WHERE kind = ? AND expires > ?",
                vec![kind.into(), (now_ms() as i64).into()],
            )?
            .to_array()?;
        self.sql()
            .exec("DELETE FROM ceremonies WHERE kind = ?", vec![kind.into()])?;
        let row = rows
            .first()
            .ok_or_else(|| Fail(400, "no ceremony in progress".into()))?;
        serde_json::from_str(&row.v).map_err(|e| Fail(500, e.to_string()))
    }

    fn new_session(&self) -> Res<String> {
        let token = new_token(&self.user());
        self.sql().exec(
            "INSERT INTO sessions (hash, expires) VALUES (?, ?)",
            vec![
                sha256_b64(&token).into(),
                ((now_ms() + SESSION_TTL_MS) as i64).into(),
            ],
        )?;
        Ok(token)
    }

    fn check_session(&self, token: &str) -> Res<bool> {
        if token.is_empty() {
            return Ok(false);
        }
        let rows: Vec<Row<i64>> = self
            .sql()
            .exec(
                "SELECT expires AS v FROM sessions WHERE hash = ?",
                vec![sha256_b64(token).into()],
            )?
            .to_array()?;
        Ok(rows.first().is_some_and(|r| r.v as u64 > now_ms()))
    }

    fn authed(&self, session: &str, f: impl FnOnce(&Self) -> Res<Value>) -> Res<Value> {
        if !self.check_session(session)? {
            return fail(401, "not signed in");
        }
        f(self)
    }

    // ---- Passkeys -------------------------------------------------------

    fn register_start(&self, origin: &str, session: &str) -> Res<Value> {
        let existing = self.passkeys()?;
        // Adding a passkey to an account that has one needs a session.
        if !existing.is_empty() && !self.check_session(session)? {
            return fail(403, "this user already has a passkey; sign in first");
        }
        let exclude = existing.iter().map(|(_, p)| p.cred_id().clone()).collect();
        let user = self.user();
        let (ccr, state) = self
            .webauthn(origin)?
            .start_passkey_registration(self.user_uuid()?, &user, &user, Some(exclude))
            .map_err(|e| Fail(400, e.to_string()))?;
        self.put_ceremony("register", &state)?;
        Ok(serde_json::to_value(ccr).unwrap())
    }

    fn register_finish(&self, origin: &str, body: &Value) -> Res<Value> {
        let state: PasskeyRegistration = self.take_ceremony("register")?;
        let cred: RegisterPublicKeyCredential = serde_json::from_value(body["credential"].clone())
            .map_err(|e| Fail(400, e.to_string()))?;
        let passkey = self
            .webauthn(origin)?
            .finish_passkey_registration(&cred, &state)
            .map_err(|e| Fail(400, e.to_string()))?;
        self.sql().exec(
            "INSERT INTO passkeys (v) VALUES (?)",
            vec![serde_json::to_string(&passkey).unwrap().into()],
        )?;
        Ok(json!({ "session": self.new_session()? }))
    }

    fn login_start(&self, origin: &str) -> Res<Value> {
        let keys: Vec<Passkey> = self.passkeys()?.into_iter().map(|(_, p)| p).collect();
        if keys.is_empty() {
            // A real server must not reveal this (decision 0004); fine here.
            return fail(400, "no passkey registered");
        }
        let (rcr, state) = self
            .webauthn(origin)?
            .start_passkey_authentication(&keys)
            .map_err(|e| Fail(400, e.to_string()))?;
        self.put_ceremony("login", &state)?;
        Ok(serde_json::to_value(rcr).unwrap())
    }

    fn login_finish(&self, origin: &str, body: &Value) -> Res<Value> {
        let state: PasskeyAuthentication = self.take_ceremony("login")?;
        let cred: PublicKeyCredential = serde_json::from_value(body["credential"].clone())
            .map_err(|e| Fail(400, e.to_string()))?;
        let result = self
            .webauthn(origin)?
            .finish_passkey_authentication(&cred, &state)
            .map_err(|e| Fail(401, e.to_string()))?;
        for (id, mut key) in self.passkeys()? {
            if key.update_credential(&result) == Some(true) {
                self.sql().exec(
                    "UPDATE passkeys SET v = ? WHERE id = ?",
                    vec![serde_json::to_string(&key).unwrap().into(), id.into()],
                )?;
            }
        }
        Ok(json!({ "session": self.new_session()? }))
    }

    // ---- Data and push --------------------------------------------------

    fn routines(&self) -> Res<Vec<Value>> {
        #[derive(Deserialize, serde::Serialize)]
        struct R {
            id: i64,
            name: String,
            created: i64,
        }
        let rows: Vec<R> = self
            .sql()
            .exec("SELECT id, name, created FROM routines ORDER BY id", None)?
            .to_array()?;
        Ok(rows
            .into_iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect())
    }

    fn list_routines(&self) -> Res<Value> {
        Ok(json!({ "routines": self.routines()? }))
    }

    fn add_routine(&self, body: &Value) -> Res<Value> {
        let name = body["name"].as_str().unwrap_or("").trim();
        if name.is_empty() || name.len() > 200 {
            return fail(400, "name must be 1-200 characters");
        }
        self.sql().exec(
            "INSERT INTO routines (name, created) VALUES (?, ?)",
            vec![name.into(), (now_ms() as i64).into()],
        )?;
        let routines = self.routines()?;
        // Push to every connected device. The sockets were accepted with the
        // hibernation API, so this works even after the object was evicted
        // from memory and woken by this request.
        let msg =
            json!({ "type": "routines", "routines": routines, "born": self.born }).to_string();
        let sockets = self.state.get_websockets();
        for ws in &sockets {
            let _ = ws.send_with_str(&msg);
        }
        Ok(json!({ "routines": routines, "pushedTo": sockets.len() }))
    }

    fn websocket(&self, session: &str) -> Result<Response> {
        if !self.check_session(session).unwrap_or(false) {
            return Response::error("not signed in", 401);
        }
        let pair = WebSocketPair::new()?;
        self.state.accept_web_socket(&pair.server);
        Response::from_websocket(pair.client)
    }

    // ---- OAuth ----------------------------------------------------------

    fn issue_code(&self, body: &Value) -> Res<Value> {
        let code = new_token(&self.user());
        let grant = json!({
            "client_id": body["client_id"], "redirect_uri": body["redirect_uri"],
            "challenge": body["code_challenge"], "resource": body["resource"], "scope": body["scope"],
        });
        self.sql().exec(
            "INSERT INTO oauth_codes (hash, v, expires) VALUES (?, ?, ?)",
            vec![
                sha256_b64(&code).into(),
                grant.to_string().into(),
                ((now_ms() + CODE_TTL_MS) as i64).into(),
            ],
        )?;
        Ok(json!({ "code": code }))
    }

    /// `POST /token` with `grant_type=authorization_code`. Errors are
    /// OAuth's `invalid_grant`, or `invalid_target` for a wrong `resource`.
    fn redeem_code(&self, body: &Value) -> Res<Value> {
        let invalid = || fail(400, "invalid_grant");
        let client_id = body["client_id"].as_str().unwrap_or("");
        let code = body["code"].as_str().unwrap_or("");
        let hash = sha256_b64(code);
        let rows: Vec<Row<String>> = self
            .sql()
            .exec(
                "SELECT v FROM oauth_codes WHERE hash = ? AND expires > ?",
                vec![hash.clone().into(), (now_ms() as i64).into()],
            )?
            .to_array()?;
        // Single use: delete before checking anything else.
        self.sql()
            .exec("DELETE FROM oauth_codes WHERE hash = ?", vec![hash.into()])?;
        let Some(row) = rows.first() else {
            return invalid();
        };
        let grant: Value = serde_json::from_str(&row.v).unwrap();
        let verifier = body["code_verifier"].as_str().unwrap_or("");
        if grant["client_id"] != client_id
            || grant["redirect_uri"].as_str() != body["redirect_uri"].as_str()
            || verifier.len() < 43
            || grant["challenge"].as_str() != Some(&pkce_s256(verifier))
        {
            return invalid();
        }
        let resource = grant["resource"].as_str().unwrap_or("");
        // RFC 8707: a resource named here must be the one authorized.
        if body["resource"].as_str().is_some_and(|r| r != resource) {
            return fail(400, "invalid_target");
        }
        let scope = grant["scope"].as_str().unwrap_or("");
        self.issue_tokens(client_id, resource, scope, &random_secret())
    }

    /// `POST /token` with `grant_type=refresh_token`: rotates the token.
    fn refresh(&self, body: &Value) -> Res<Value> {
        let invalid = || fail(400, "invalid_grant");
        let client_id = body["client_id"].as_str().unwrap_or("");
        let token = body["refresh_token"].as_str().unwrap_or("");
        let hash = sha256_b64(token);
        #[derive(Deserialize)]
        struct T {
            family: String,
            client_id: String,
            resource: String,
            scope: String,
            expires: i64,
            used: i64,
        }
        let rows: Vec<T> = self
                    .sql()
                    .exec(
                        "SELECT family, client_id, resource, scope, expires, used FROM oauth_tokens WHERE hash = ? AND kind = 'refresh'",
                        vec![hash.clone().into()],
                    )?
                    .to_array()?;
        let Some(t) = rows.first() else {
            return invalid();
        };
        if t.used != 0 {
            // Reuse of a rotated refresh token: revoke the whole family.
            self.sql().exec(
                "DELETE FROM oauth_tokens WHERE family = ?",
                vec![t.family.clone().into()],
            )?;
            return invalid();
        }
        if t.client_id != client_id || (t.expires as u64) < now_ms() {
            return invalid();
        }
        self.sql().exec(
            "UPDATE oauth_tokens SET used = 1 WHERE hash = ?",
            vec![hash.into()],
        )?;
        self.issue_tokens(&t.client_id, &t.resource, &t.scope, &t.family)
    }

    fn issue_tokens(
        &self,
        client_id: &str,
        resource: &str,
        scope: &str,
        family: &str,
    ) -> Res<Value> {
        let access = new_token(&self.user());
        let refresh = new_token(&self.user());
        for (token, kind, ttl) in [
            (&access, "access", ACCESS_TTL_MS),
            (&refresh, "refresh", REFRESH_TTL_MS),
        ] {
            self.sql().exec(
                "INSERT INTO oauth_tokens (hash, kind, family, client_id, resource, scope, expires) VALUES (?, ?, ?, ?, ?, ?, ?)",
                vec![
                    sha256_b64(token).into(),
                    kind.into(),
                    family.into(),
                    client_id.into(),
                    resource.into(),
                    scope.into(),
                    ((now_ms() + ttl) as i64).into(),
                ],
            )?;
        }
        Ok(json!({
            "access_token": access, "token_type": "Bearer", "expires_in": ACCESS_TTL_MS / 1000,
            "refresh_token": refresh, "scope": scope,
        }))
    }

    fn access_token_ok(&self, token: &str, resource: &str) -> Res<bool> {
        let rows: Vec<Row<i64>> = self
            .sql()
            .exec(
                "SELECT expires AS v FROM oauth_tokens WHERE hash = ? AND kind = 'access' AND resource = ?",
                vec![sha256_b64(token).into(), resource.into()],
            )?
            .to_array()?;
        Ok(rows.first().is_some_and(|r| r.v as u64 > now_ms()))
    }

    fn mcp(&self, origin: &str, req: &Request, body: &Value) -> Result<Response> {
        let token = req.headers().get("x-token")?.unwrap_or_default();
        let resource = mcp_resource(origin);
        if !self.access_token_ok(&token, &resource).unwrap_or(false) {
            return Response::error("invalid token", 401);
        }
        let version = req.headers().get("x-mcp-protocol-version")?;
        let routines = self.routines().map_err(|Fail(_, m)| Error::RustError(m))?;
        match mcp::handle(body, version.as_deref(), &routines) {
            Some(reply) => Response::from_json(&reply),
            None => Ok(Response::empty()?.with_status(202)),
        }
    }
}

pub fn pkce_s256(verifier: &str) -> String {
    sha256_b64(verifier)
}
