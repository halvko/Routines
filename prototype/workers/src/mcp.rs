//! A small MCP server over Streamable HTTP, JSON responses only, using
//! `rmcp`'s protocol types without its transport (which needs a Tokio
//! runtime Workers does not have; decision 0005).
//!
//! The tools are clients of the user's sync log like any device: a write
//! from Claude is appended to the same log, gets the next number, and pokes
//! every connected device. That is what makes MCP first class here: there is
//! no second write path to keep in step.

use rmcp::model::*;
use serde_json::{Value, json};
use worker::*;
use worker::{Request, Response, Result};

use crate::user::{Mutation, PULL_LIMIT, PullResponse, PushRequest, PushResponse, pull_url};

/// The client id MCP writes use in the sync log.
const MCP_CLIENT: &str = "mcp";

pub async fn handle(mut req: Request, env: &Env, user: &str) -> Result<Response> {
    let body = req.text().await?;
    let message: ClientJsonRpcMessage = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            let err = ServerJsonRpcMessage::error(
                ErrorData::new(ErrorCode::PARSE_ERROR, e.to_string(), None),
                None,
            );
            return json_response(&err, 400);
        }
    };

    let (request, id) = match message {
        JsonRpcMessage::Request(r) => (r.request, r.id),
        // Notifications (e.g. notifications/initialized) and responses get
        // 202 Accepted with no body, per Streamable HTTP.
        _ => return Ok(Response::empty()?.with_status(202)),
    };

    let reply = match dispatch(request, env, user).await {
        Ok(result) => ServerJsonRpcMessage::response(result, id),
        Err(error) => ServerJsonRpcMessage::error(error, Some(id)),
    };
    json_response(&reply, 200)
}

fn json_response(message: &ServerJsonRpcMessage, status: u16) -> Result<Response> {
    Ok(Response::from_json(message)?.with_status(status))
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities::builder().enable_tools().build()
}

fn server_info() -> Implementation {
    Implementation::new("routines-prototype", env!("CARGO_PKG_VERSION"))
}

const INSTRUCTIONS: &str = "Prototype Routines server. Tools read and append to the user's sync log.";

async fn dispatch(
    request: ClientRequest,
    env: &Env,
    user: &str,
) -> std::result::Result<ServerResult, ErrorData> {
    match request {
        // Older protocol era: an initialize handshake. Answer with the
        // client's version when we know it, else our newest that has one.
        ClientRequest::InitializeRequest(r) => {
            let asked = r.params.protocol_version.clone();
            let version = if KNOWN.contains(&asked) {
                asked
            } else {
                ProtocolVersion::LATEST_WITH_INITIALIZE
            };
            Ok(ServerResult::InitializeResult(
                InitializeResult::new(capabilities())
                    .with_protocol_version(version)
                    .with_server_info(server_info())
                    .with_instructions(INSTRUCTIONS),
            ))
        }
        // 2026-07-28 era: stateless, discovered per request.
        ClientRequest::DiscoverRequest(_) => {
            let mut result = DiscoverResult::new(KNOWN.to_vec(), capabilities());
            result.instructions = Some(INSTRUCTIONS.into());
            result.set_server_info(server_info());
            Ok(ServerResult::DiscoverResult(result))
        }
        ClientRequest::PingRequest(_) => Ok(ServerResult::empty(())),
        ClientRequest::ListToolsRequest(_) => {
            Ok(ServerResult::ListToolsResult(ListToolsResult::with_all_items(tools())))
        }
        ClientRequest::CallToolRequest(r) => {
            let args = r.params.arguments.unwrap_or_default();
            let result = call_tool(&r.params.name, &args, env, user).await?;
            Ok(ServerResult::CallToolResult(result))
        }
        other => Err(ErrorData::new(
            ErrorCode::METHOD_NOT_FOUND,
            format!("{} is not supported", other.method()),
            None,
        )),
    }
}

const KNOWN: [ProtocolVersion; 4] = [
    ProtocolVersion::V_2026_07_28,
    ProtocolVersion::V_2025_11_25,
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_03_26,
];

fn schema(value: Value) -> std::sync::Arc<JsonObject> {
    match value {
        Value::Object(map) => std::sync::Arc::new(map),
        _ => unreachable!("schemas are objects"),
    }
}

fn tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "read_log",
            "Read the user's sync log: every change from any device or from Claude, in order.",
            schema(json!({
                "type": "object",
                "properties": {
                    "after": { "type": "integer", "description": "Return entries after this number. Default 0." },
                    "limit": { "type": "integer", "description": "At most this many entries. Default 500." }
                }
            })),
        ),
        Tool::new(
            "add_todo",
            "Add a to-do to the user's backlog. It reaches the user's devices like a change made on one of them.",
            schema(json!({
                "type": "object",
                "properties": { "title": { "type": "string" } },
                "required": ["title"]
            })),
        ),
    ]
}

async fn call_tool(
    name: &str,
    args: &JsonObject,
    env: &Env,
    user: &str,
) -> std::result::Result<CallToolResult, ErrorData> {
    let internal = |e: worker::Error| ErrorData::internal_error(e.to_string(), None);
    match name {
        "read_log" => {
            let after = args.get("after").and_then(Value::as_i64).unwrap_or(0);
            let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(PULL_LIMIT);
            let url = pull_url(after, limit);
            let pulled: PullResponse = crate::user_stub(env, user)
                .map_err(internal)?
                .fetch_with_str(&url)
                .await
                .map_err(internal)?
                .json()
                .await
                .map_err(internal)?;
            let text = serde_json::to_string_pretty(&pulled).unwrap_or_default();
            Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
        }
        "add_todo" => {
            let Some(title) = args.get("title").and_then(Value::as_str) else {
                return Ok(CallToolResult::error(vec![ContentBlock::text("title is required")]));
            };
            let body = PushRequest {
                client_id: MCP_CLIENT.into(),
                mutations: vec![Mutation {
                    client_seq: None,
                    name: "addTodo".into(),
                    args: json!({ "title": title }),
                }],
            };
            let mut init = RequestInit::new();
            let headers = Headers::new();
            headers.set("Content-Type", "application/json").map_err(internal)?;
            init.with_method(Method::Post)
                .with_headers(headers)
                .with_body(Some(
                    serde_json::to_string(&body)
                        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
                        .into(),
                ));
            let req = Request::new_with_init("https://user/push", &init).map_err(internal)?;
            let pushed: PushResponse = crate::user_stub(env, user)
                .map_err(internal)?
                .fetch_with_request(req)
                .await
                .map_err(internal)?
                .json()
                .await
                .map_err(internal)?;
            let seq = pushed.results.first().map(|r| r.seq).unwrap_or(0);
            Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "Added \"{title}\" as change #{seq}."
            ))]))
        }
        _ => Err(ErrorData::invalid_params(format!("unknown tool {name}"), None)),
    }
}
