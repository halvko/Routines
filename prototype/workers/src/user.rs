//! One Durable Object per user: the single writer that numbers that user's
//! sync log (decision 0003), stores it in the object's own SQLite database,
//! and pokes the user's connected devices over hibernating WebSockets.

use serde::{Deserialize, Serialize};
use worker::*;

/// Wire shapes of the object's internal API, shared with `mcp.rs` so both
/// sides change together.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub client_id: String,
    pub mutations: Vec<Mutation>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mutation {
    /// The device's own counter. `None` asks the server to take the client's
    /// next number; only server-side clients such as MCP do that.
    pub client_seq: Option<i64>,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub client_seq: i64,
    pub seq: i64,
    /// True when `(clientId, clientSeq)` was already in the log: a retry,
    /// answered with the number it got the first time.
    pub duplicate: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResponse {
    pub results: Vec<PushResult>,
    pub last_seq: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub seq: i64,
    pub client_id: String,
    pub client_seq: i64,
    pub name: String,
    pub args: serde_json::Value,
    pub at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    pub events: Vec<Event>,
    pub last_seq: i64,
}

/// Default page size for pull, for devices and MCP alike.
pub const PULL_LIMIT: i64 = 500;

/// The object's URL for a pull, used by the router's callers inside the Worker.
pub fn pull_url(after: i64, limit: i64) -> String {
    format!("https://user/pull?after={after}&limit={limit}")
}

#[derive(Deserialize)]
struct Row {
    seq: i64,
    client_id: String,
    client_seq: i64,
    name: String,
    args: String,
    at: i64,
}

#[derive(Deserialize)]
struct Num {
    n: Option<i64>,
}

#[durable_object]
pub struct User {
    state: State,
    env: Env,
}

impl DurableObject for User {
    fn new(state: State, env: Env) -> Self {
        console_error_panic_hook::set_once();
        let sql = state.storage().sql();
        // Runs on every wake; cheap and idempotent. A real server would keep
        // a schema version and numbered migrations.
        sql.exec(
            "CREATE TABLE IF NOT EXISTS log (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                client_id TEXT NOT NULL,
                client_seq INTEGER NOT NULL,
                name TEXT NOT NULL,
                args TEXT NOT NULL,
                at INTEGER NOT NULL,
                UNIQUE (client_id, client_seq)
            )",
            None,
        )
        .expect("create log table");
        // Answer keepalive pings without waking the object (hibernation).
        state.set_websocket_auto_response(&worker_sys::WebSocketRequestResponsePair::new(
            "ping", "pong",
        )
        .expect("auto response pair"));
        Self { state, env }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        match (req.method(), url.path()) {
            (Method::Post, "/push") => {
                let body: PushRequest = req.json().await?;
                Response::from_json(&self.push(body)?)
            }
            (Method::Get, "/pull") => {
                let mut after = 0;
                let mut limit = PULL_LIMIT;
                for (k, v) in url.query_pairs() {
                    match &*k {
                        "after" => after = v.parse().unwrap_or(0),
                        "limit" => limit = v.parse::<i64>().unwrap_or(PULL_LIMIT).clamp(1, 1000),
                        _ => {}
                    }
                }
                Response::from_json(&self.pull(after, limit)?)
            }
            (Method::Get, "/ws") => {
                let upgrade = req.headers().get("Upgrade")?;
                if upgrade.as_deref() != Some("websocket") {
                    return Response::error("expected a WebSocket upgrade", 426);
                }
                let pair = WebSocketPair::new()?;
                // Hibernating accept: the object can be evicted from memory
                // while sockets stay open, and is woken for messages.
                self.state.accept_web_socket(&pair.server);
                Response::from_websocket(pair.client)
            }
            (Method::Post, path) if path.starts_with("/passkey/") => {
                // Objects are only ever addressed by name (the user id).
                let Some(user) = self.state.id().name() else {
                    return Response::error("object has no name", 500);
                };
                let step = path.trim_start_matches("/passkey/").to_string();
                crate::passkey::handle(&self.state, &self.env, &user, req, &step).await
            }
            _ => Response::error("not found", 404),
        }
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        // Devices only listen; anything else gets the current head so a
        // reconnecting device knows whether to pull.
        if let WebSocketIncomingMessage::String(_) = message {
            ws.send_with_str(poke(self.last_seq()?))?;
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        // Code 1005 ("no status") may not be sent back.
        let code = if code == 1005 { 1000 } else { code as u16 };
        let _ = ws.close(Some(code), Some("bye"));
        Ok(())
    }
}

fn poke(last_seq: i64) -> String {
    serde_json::json!({ "type": "poke", "lastSeq": last_seq }).to_string()
}

impl User {
    /// Appends each mutation once. All statements run synchronously with no
    /// `await` between them, so the Durable Object commits them together
    /// and confirms nothing to the device before they are durable.
    fn push(&self, body: PushRequest) -> Result<PushResponse> {
        let sql = self.state.storage().sql();
        let now = Date::now().as_millis() as i64;
        let mut results = Vec::with_capacity(body.mutations.len());
        let mut appended = false;

        for m in body.mutations {
            // This is where the server runs the shared core on the change
            // (0003). The prototype stores it as given.
            let client_seq = match m.client_seq {
                Some(n) => {
                    let existing = sql
                        .exec(
                            "SELECT seq AS n FROM log WHERE client_id = ? AND client_seq = ?",
                            vec![body.client_id.as_str().into(), n.into()],
                        )?
                        .to_array::<Num>()?;
                    if let Some(Num { n: Some(seq) }) = existing.into_iter().next() {
                        results.push(PushResult { client_seq: n, seq, duplicate: true });
                        continue;
                    }
                    n
                }
                // Server-numbered (MCP): a fresh number, so never a duplicate.
                // A retried MCP call therefore appends twice; the real server
                // needs an idempotency key per tool call (see the findings).
                None => {
                    sql.exec(
                        "SELECT MAX(client_seq) AS n FROM log WHERE client_id = ?",
                        vec![body.client_id.as_str().into()],
                    )?
                    .one::<Num>()?
                    .n
                    .unwrap_or(0)
                        + 1
                }
            };
            let seq = sql
                .exec(
                    "INSERT INTO log (client_id, client_seq, name, args, at)
                     VALUES (?, ?, ?, ?, ?) RETURNING seq AS n",
                    vec![
                        body.client_id.as_str().into(),
                        client_seq.into(),
                        m.name.into(),
                        m.args.to_string().into(),
                        now.into(),
                    ],
                )?
                .one::<Num>()?
                .n
                .ok_or("insert returned no seq")?;
            results.push(PushResult { client_seq, seq, duplicate: false });
            appended = true;
        }

        let last_seq = self.last_seq()?;
        if appended {
            let msg = poke(last_seq);
            for ws in self.state.get_websockets() {
                // A socket that is closing is not an error for the push.
                let _ = ws.send_with_str(&msg);
            }
        }
        Ok(PushResponse { results, last_seq })
    }

    fn pull(&self, after: i64, limit: i64) -> Result<PullResponse> {
        let rows = self
            .state
            .storage()
            .sql()
            .exec(
                "SELECT seq, client_id, client_seq, name, args, at FROM log
                 WHERE seq > ? ORDER BY seq LIMIT ?",
                vec![after.into(), limit.into()],
            )?
            .to_array::<Row>()?;
        let events = rows
            .into_iter()
            .map(|r| Event {
                seq: r.seq,
                client_id: r.client_id,
                client_seq: r.client_seq,
                name: r.name,
                args: serde_json::from_str(&r.args).unwrap_or(serde_json::Value::Null),
                at: r.at,
            })
            .collect();
        Ok(PullResponse { events, last_seq: self.last_seq()? })
    }

    fn last_seq(&self) -> Result<i64> {
        Ok(self
            .state
            .storage()
            .sql()
            .exec("SELECT MAX(seq) AS n FROM log", None)?
            .one::<Num>()?
            .n
            .unwrap_or(0))
    }
}
