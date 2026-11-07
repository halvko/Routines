//! A small stateless MCP JSON-RPC handler built on `rmcp`'s protocol types.
//! `rmcp`'s own HTTP transport needs a Tokio runtime, which Workers lacks.

use rmcp::model::{
    CallToolResult, ContentBlock, DiscoverResult, Implementation, InitializeResult, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool,
};
use serde_json::{Value, json};

fn capabilities() -> ServerCapabilities {
    ServerCapabilities::builder().enable_tools().build()
}

fn server_info() -> Implementation {
    Implementation::new("routines-prototype", "0.0.0")
}

fn list_routines_tool() -> Tool {
    let schema = json!({ "type": "object", "properties": {}, "additionalProperties": false });
    let Value::Object(schema) = schema else { unreachable!() };
    Tool::new("list_routines", "List the user's routines.", schema)
}

/// Answers one JSON-RPC message. `None` means it was a notification, which
/// gets `202 Accepted` and no body.
pub fn handle(msg: &Value, header_version: Option<&str>, routines: &[Value]) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg["method"].as_str().unwrap_or("");
    let params = &msg["params"];
    // From 2026-07-28 the version travels with every request; before, it is
    // fixed by `initialize` and repeated in the MCP-Protocol-Version header.
    let version = params["_meta"]["io.modelcontextprotocol/protocolVersion"]
        .as_str()
        .or(header_version)
        .unwrap_or(ProtocolVersion::V_2025_06_18.as_str())
        .to_string();
    let modern = !ProtocolVersion::KNOWN_VERSIONS.iter().any(|v| v.as_str() == version)
        || !known(&version).has_initialize();

    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let chosen = if known(asked).has_initialize() && is_known(asked) {
                known(asked)
            } else {
                ProtocolVersion::LATEST_WITH_INITIALIZE
            };
            let result = InitializeResult::new(capabilities())
                .with_protocol_version(chosen)
                .with_server_info(server_info());
            return Some(reply(id, legacy(serde_json::to_value(result).unwrap())));
        }
        "server/discover" => Ok(serde_json::to_value(
            DiscoverResult::new(
                vec![
                    ProtocolVersion::V_2025_06_18,
                    ProtocolVersion::V_2025_11_25,
                    ProtocolVersion::V_2026_07_28,
                ],
                capabilities(),
            )
            .with_server_info(server_info()),
        )
        .unwrap()),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(serde_json::to_value(ListToolsResult::with_all_items(vec![list_routines_tool()])).unwrap()),
        "tools/call" => match params["name"].as_str() {
            Some("list_routines") => {
                let text = serde_json::to_string_pretty(routines).unwrap();
                Ok(serde_json::to_value(CallToolResult::success(vec![ContentBlock::text(text)])).unwrap())
            }
            _ => Err((-32602, "unknown tool".into())),
        },
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(v) if modern => reply(id, v),
        Ok(v) => reply(id, legacy(v)),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    })
}

fn is_known(v: &str) -> bool {
    ProtocolVersion::KNOWN_VERSIONS.iter().any(|k| k.as_str() == v)
}

fn known(v: &str) -> ProtocolVersion {
    ProtocolVersion::KNOWN_VERSIONS
        .iter()
        .find(|k| k.as_str() == v)
        .cloned()
        .unwrap_or(ProtocolVersion::LATEST)
}

/// Older revisions don't have the 2026-07-28 result fields.
fn legacy(mut v: Value) -> Value {
    if let Value::Object(map) = &mut v {
        for key in ["resultType", "ttlMs", "cacheScope"] {
            map.remove(key);
        }
    }
    v
}

fn reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}
