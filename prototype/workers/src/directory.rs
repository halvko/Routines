//! A single Durable Object holding OAuth clients registered with DCR.
//! (In the real server it would also map email to user.)

use serde::Deserialize;
use serde_json::{Value, json};
use worker::*;

use crate::util::*;

#[durable_object]
pub struct Directory {
    state: State,
}

impl DurableObject for Directory {
    fn new(state: State, _env: Env) -> Self {
        state
            .storage()
            .sql()
            .exec("CREATE TABLE IF NOT EXISTS clients (id TEXT PRIMARY KEY, v TEXT NOT NULL, created INTEGER NOT NULL)", None)
            .expect("schema");
        Self { state }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let body: Value = req.json().await?;
        let sql = self.state.storage().sql();
        match req.path().as_str() {
            "/register" => {
                let id = random_secret();
                let mut client = body;
                client["client_id"] = json!(id);
                client["client_id_issued_at"] = json!(now_ms() / 1000);
                sql.exec(
                    "INSERT INTO clients (id, v, created) VALUES (?, ?, ?)",
                    vec![
                        id.into(),
                        client.to_string().into(),
                        (now_ms() as i64).into(),
                    ],
                )?;
                Response::from_json(&client)
            }
            "/client" => {
                #[derive(Deserialize)]
                struct Row {
                    v: String,
                }
                let id = body["client_id"].as_str().unwrap_or("");
                let rows: Vec<Row> = sql
                    .exec("SELECT v FROM clients WHERE id = ?", vec![id.into()])?
                    .to_array()?;
                match rows.first() {
                    Some(r) => Response::ok(r.v.clone()),
                    None => Response::error("unknown client", 404),
                }
            }
            _ => Response::error("not found", 404),
        }
    }
}
