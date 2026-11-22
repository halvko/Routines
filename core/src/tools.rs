//! The MCP tool surface, generated from [`Change`] so the two cannot drift:
//! one tool per change (its name is the change's `op` in snake_case, its
//! description the change's doc comment, its input schema the change's
//! fields), plus read-only tools for what the app shows.
//!
//! A tool call becomes a change with [`change_from_tool`], and goes through
//! the user's [`crate::Log`] like a push from any device. [`tool_call`] is the
//! reverse, for handing a change's Undo back to Claude.

use crate::change::Change;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// One MCP tool, as `tools/list` describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub annotations: Annotations,
}

/// MCP tool annotations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
}

/// Read-only tools: name and description. Their answers are
/// [`crate::today`], [`crate::open_backlog`], the live routines and
/// [`crate::deleted`].
pub const READ_TOOLS: &[(&str, &str)] = &[
    (
        "today",
        "What can be done now, what is coming up (and what it waits on), and every unfinished run.",
    ),
    (
        "backlog",
        "To-dos not done yet: starred first, then newest first.",
    ),
    (
        "routines",
        "Every routine with its steps, what each step comes after, and the options (flags) it can be started with.",
    ),
    (
        "deleted",
        "Deleted routines and to-dos and abandoned runs, newest first, with when and by which device. Bring one back with restore_routine, restore_todo or restore_run.",
    ),
];

/// Every tool, read-only ones first.
pub fn tools() -> Vec<Tool> {
    let mut out: Vec<Tool> = READ_TOOLS
        .iter()
        .map(|(name, description)| Tool {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
            annotations: Annotations {
                read_only_hint: true,
                destructive_hint: false,
            },
        })
        .collect();
    let root = serde_json::to_value(schemars::schema_for!(Change)).expect("schema is JSON");
    let defs = root.get("$defs").cloned();
    for variant in root["oneOf"].as_array().expect("Change is a tagged enum") {
        let op = variant["properties"]["op"]["const"]
            .as_str()
            .expect("every change has an op");
        // Set on the variant with `#[schemars(extend("x-destructive" = true))]`.
        let destructive = variant.get("x-destructive") == Some(&Value::Bool(true));
        let mut props = variant["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        props.remove("op");
        let required: Vec<Value> = variant["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r.as_str() != Some("op"))
            .cloned()
            .collect();
        let mut schema = Map::new();
        schema.insert("type".into(), "object".into());
        schema.insert("properties".into(), Value::Object(props));
        schema.insert("required".into(), Value::Array(required));
        if let Some(defs) = &defs {
            schema.insert("$defs".into(), defs.clone());
        }
        out.push(Tool {
            name: snake(op),
            description: variant["description"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            input_schema: Value::Object(schema),
            annotations: Annotations {
                read_only_hint: false,
                destructive_hint: destructive,
            },
        });
    }
    out
}

/// The change a call of tool `name` asks for. Fails for read-only tools,
/// unknown names and arguments that do not fit.
pub fn change_from_tool(name: &str, args: Value) -> Result<Change, serde_json::Error> {
    let mut obj = match args {
        Value::Object(o) => o,
        Value::Null => Map::new(),
        other => return serde_json::from_value(other),
    };
    obj.insert("op".into(), Value::String(camel(name)));
    serde_json::from_value(Value::Object(obj))
}

/// A change as a tool call (name and arguments), the reverse of
/// [`change_from_tool`]. Use it to hand a change's Undo back to Claude, e.g.
/// `remove_step` returns a `restore_step` call.
pub fn tool_call(change: &Change) -> (String, Value) {
    let mut args = serde_json::to_value(change).expect("a change is JSON");
    let op = args
        .as_object_mut()
        .and_then(|o| o.remove("op"))
        .and_then(|op| op.as_str().map(snake))
        .expect("every change has an op");
    (op, args)
}

fn snake(camel: &str) -> String {
    let mut s = String::new();
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            s.push('_');
            s.push(c.to_ascii_lowercase());
        } else {
            s.push(c);
        }
    }
    s
}

fn camel(snake: &str) -> String {
    let mut s = String::new();
    let mut up = false;
    for c in snake.chars() {
        if c == '_' {
            up = true;
        } else if up {
            s.push(c.to_ascii_uppercase());
            up = false;
        } else {
            s.push(c);
        }
    }
    s
}
