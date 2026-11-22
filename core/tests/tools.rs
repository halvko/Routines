//! The MCP tool list is generated from the changes; check it is complete and
//! that every tool's arguments turn back into its change.

use routines_core::*;
use serde_json::{Value, json};
use std::collections::HashSet;

#[test]
fn every_change_is_a_tool_with_a_description() {
    let tools = tools();
    let names: HashSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names.len(), tools.len(), "names are unique");
    for t in &tools {
        assert!(!t.description.is_empty(), "{} has a description", t.name);
        assert_eq!(t.input_schema["type"], "object");
        assert!(
            t.input_schema["properties"].get("op").is_none(),
            "{}: the op is the tool name, not an argument",
            t.name
        );
    }
    for name in [
        "today",
        "backlog",
        "routines",
        "start_run",
        "mark_step",
        "place_run",
        "add_item",
        "pull_item",
        "add_task",
        "add_dep",
        "delete_routine",
        "restore_run",
    ] {
        assert!(names.contains(name), "{name} is a tool");
    }
    let start = tools.iter().find(|t| t.name == "start_run").unwrap();
    assert!(
        start.input_schema["properties"].get("nodes").is_none(),
        "Claude never sends a run's steps; the core makes them"
    );
    assert_eq!(start.input_schema["required"], json!(["routine"]));
    let destructive: Vec<_> = tools
        .iter()
        .filter(|t| t.annotations.destructive_hint)
        .map(|t| t.name.as_str())
        .collect();
    assert!(destructive.contains(&"delete_item"));
    assert!(!destructive.contains(&"mark_step"));
    assert!(
        tools
            .iter()
            .filter(|t| t.annotations.read_only_hint)
            .count()
            == READ_TOOLS.len()
    );
}

#[test]
fn tool_arguments_become_changes() {
    let c = change_from_tool(
        "place_run",
        json!({"run": "a", "place": {"rel": "after", "run": "b", "step": "wake"}}),
    )
    .unwrap();
    assert_eq!(
        c,
        Change::PlaceRun {
            run: "a".into(),
            place: Some(Placement {
                rel: Rel::After,
                run: "b".into(),
                step: "wake".into()
            })
        }
    );
    assert!(change_from_tool("today", Value::Null).is_err());
    assert!(change_from_tool("mark_step", json!({"run": "a"})).is_err());
}

#[test]
fn changes_round_trip_as_json() {
    let c = json!({"op": "markStep", "run": "r1", "step": "wake", "mark": "done"});
    let parsed: Change = serde_json::from_value(c.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), c);
}
