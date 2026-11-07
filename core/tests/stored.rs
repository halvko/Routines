//! Data the web app has saved must load, and save back unchanged.
//! `fixtures/stored.json` was captured from the app (`localStorage` key
//! `routines.v1`) after using every feature that writes data: finished and
//! running runs, flags, runs placed before and after steps, backlog items
//! starred, pulled into Today and done.

use routines_core::*;
use serde_json::{Value, json};

fn stored() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/stored.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn saved_data_loads_and_saves_back_unchanged() {
    let raw = stored();
    let db: Db = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(&db).unwrap(), raw);
    // Runs and routines keep their order, which placing follows.
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(
        db.runs.keys().cloned().collect::<Vec<_>>(),
        keys(&raw["runs"])
    );
    assert_eq!(
        db.routines.keys().cloned().collect::<Vec<_>>(),
        keys(&raw["routines"])
    );
}

#[test]
fn saved_data_means_the_same() {
    let db: Db = serde_json::from_value(stored()).unwrap();
    let morning = db.runs.values().find(|r| r.name == "Morning").unwrap();
    assert_eq!(morning.flags.get("gym"), Some(&true));
    assert_eq!(morning.done.get("wake"), Some(&Mark::Done));
    assert!(!is_finished(morning));
    let travel = db.runs.values().find(|r| r.name == "Travel").unwrap();
    let attach = travel.attach.as_ref().unwrap();
    assert_eq!(
        (attach.rel, attach.run.as_str()),
        (Rel::After, morning.id.as_str())
    );
    let breakfast = db.runs.values().find(|r| r.name == "Breakfast").unwrap();
    assert!(is_finished(breakfast) && breakfast.finished_at.is_some());
    let todo = db
        .runs
        .values()
        .find(|r| r.kind.as_deref() == Some("todo"))
        .unwrap();
    let item = db
        .backlog
        .iter()
        .find(|i| i.id == *todo.backlog_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(item.run_id.as_ref(), Some(&todo.id));
    assert_eq!(
        db.routines["travel"].steps[0].when.as_deref(),
        Some("flying")
    );
}

#[test]
fn data_saved_before_the_backlog_and_preferences_loads() {
    let mut raw = stored();
    let obj = raw.as_object_mut().unwrap();
    obj.remove("backlog");
    obj.remove("prefs");
    let db: Db = serde_json::from_value(raw).unwrap();
    assert!(db.backlog.is_empty());
    assert!(db.prefs.is_none());
}

#[test]
fn fields_this_version_does_not_know_are_kept() {
    let mut raw = stored();
    raw["future"] = json!({ "a": 1 });
    let run = raw["runs"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    run["category"] = json!("green");
    run["nodes"][0]["timing"] = json!("day before");
    raw["routines"]["travel"]["steps"][0]["after"] = json!([{ "id": "x", "sortKey": "a0" }]);
    let db: Db = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(&db).unwrap(), raw);
}
