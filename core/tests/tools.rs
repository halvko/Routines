//! The MCP tool list is generated from the changes; check it is complete,
//! that every change tool has a scenario, and that every tool's arguments
//! turn back into its change.

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
        "add_todo",
        "pull_todo",
        "deleted",
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
    assert!(start.input_schema["properties"].get("name").is_none());
    assert!(
        names.contains("restore_step"),
        "a step removed through MCP can be put back"
    );
    assert_eq!(start.input_schema["required"], json!(["routine"]));
    let destructive: HashSet<_> = tools
        .iter()
        .filter(|t| t.annotations.destructive_hint)
        .map(|t| t.name.as_str())
        .collect();
    let expected: HashSet<_> = [
        "delete_routine",
        "remove_step",
        "remove_dep",
        "abandon_run",
        "unpull_todo",
        "delete_todo",
    ]
    .into();
    assert_eq!(destructive, expected);
    for t in &tools {
        assert!(
            t.input_schema.get("x-destructive").is_none(),
            "{}: the hint is an annotation, not part of the input schema",
            t.name
        );
    }
    assert!(
        tools
            .iter()
            .filter(|t| t.annotations.read_only_hint)
            .count()
            == READ_TOOLS.len()
    );
}

/// Every change in the scenarios (`tests/changes/*.json`), as JSON.
fn scenario_changes() -> Vec<Value> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/changes");
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap();
        for case in file["cases"].as_array().unwrap() {
            for step in case["steps"].as_array().unwrap() {
                out.push(step["change"].clone());
            }
        }
    }
    out
}

fn snake(camel: &str) -> String {
    camel.chars().fold(String::new(), |mut s, c| {
        if c.is_ascii_uppercase() {
            s.push('_');
        }
        s.push(c.to_ascii_lowercase());
        s
    })
}

#[test]
fn every_tool_is_used_in_the_scenarios_and_its_arguments_become_its_change() {
    let tools = tools();
    let change_tools: HashSet<String> = tools
        .iter()
        .filter(|t| !t.annotations.read_only_hint)
        .map(|t| t.name.clone())
        .collect();
    let mut used = HashSet::new();
    for c in scenario_changes() {
        let op = c["op"].as_str().unwrap().to_string();
        let name = snake(&op);
        assert!(
            change_tools.contains(&name),
            "{op} is offered as tool {name}"
        );
        let mut args = c.clone();
        args.as_object_mut().unwrap().remove("op");
        let from_tool = change_from_tool(&name, args).unwrap();
        let direct: Change = serde_json::from_value(c.clone()).unwrap();
        assert_eq!(from_tool, direct, "{name}");
        assert_eq!(tool_call(&direct).0, name);
        assert_eq!(
            change_from_tool(&name, tool_call(&direct).1).unwrap(),
            direct
        );
        used.insert(name);
    }
    let unused: Vec<_> = change_tools.difference(&used).collect();
    assert!(unused.is_empty(), "tools with no scenario: {unused:?}");
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
fn a_step_removed_through_mcp_comes_back_through_its_undo() {
    let mut db = seed();
    let stamp = |seq| Stamp {
        client: "mcp".into(),
        seq,
        at: 1,
    };
    let removed = apply(
        &mut db,
        &stamp(1),
        &change_from_tool(
            "remove_step",
            json!({"routine": "breakfast", "step": "eat"}),
        )
        .unwrap(),
    )
    .unwrap();
    let (name, args) = tool_call(&removed.undo.unwrap());
    assert_eq!(name, "restore_step");
    apply(&mut db, &stamp(2), &change_from_tool(&name, args).unwrap()).unwrap();
    assert_eq!(db.routines, seed().routines);
}

#[test]
fn changes_round_trip_as_json() {
    let c = json!({"op": "markStep", "run": "r1", "step": "wake", "mark": "done"});
    let parsed: Change = serde_json::from_value(c.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), c);
}
