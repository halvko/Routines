//! Runs the change scenarios in `tests/changes/*.json`: a starting state,
//! a list of changes with what each should do, and what the data (and
//! Today) should look like at the end. They pin how every change behaves,
//! for the web app, the server and MCP alike, written as data so other
//! implementations (the TypeScript shell's tests, say) can read them too.
//!
//! Expected values are matched as a subset: objects need only the keys
//! given (`null` means "absent or null"), arrays must have the same length
//! with each element matching.

use routines_core::*;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct File {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    /// `"seed"`, or stored data.
    start: Value,
    steps: Vec<Step>,
    /// A subset of the data after the last step.
    #[serde(default)]
    expected: Option<Value>,
    /// A subset of `today()` after the last step.
    #[serde(default)]
    today: Option<Value>,
    /// A subset of `open_backlog()` after the last step.
    #[serde(default)]
    backlog: Option<Value>,
    /// A subset of `deleted()` after the last step.
    #[serde(default)]
    deleted: Option<Value>,
}

#[derive(Deserialize)]
struct Step {
    change: Value,
    /// A subset of [`Applied`], when the change should apply.
    #[serde(default)]
    ok: Option<Value>,
    /// A subset of [`Rejected`], when it should be refused.
    #[serde(default)]
    rejected: Option<Value>,
}

/// Does `actual` contain `expected`? Returns where it differs.
fn subset(expected: &Value, actual: &Value, path: &str) -> Result<(), String> {
    match (expected, actual) {
        (Value::Null, Value::Null) => Ok(()),
        (Value::Object(e), Value::Object(a)) => {
            for (k, ev) in e {
                let p = format!("{path}.{k}");
                match a.get(k) {
                    Some(av) => subset(ev, av, &p)?,
                    None if ev.is_null() => {}
                    None => return Err(format!("{p}: missing, expected {ev}")),
                }
            }
            Ok(())
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Err(format!("{path}: expected {expected}, got {actual}"));
            }
            for (i, (ev, av)) in e.iter().zip(a).enumerate() {
                subset(ev, av, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ if expected == actual => Ok(()),
        _ => Err(format!("{path}: expected {expected}, got {actual}")),
    }
}

#[test]
fn change_scenarios() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/changes");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    let mut failures = Vec::new();
    let mut count = 0;
    for path in files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let file: File = serde_json::from_str(&fs::read_to_string(path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let fname = path.file_name().unwrap().to_string_lossy();
        for case in file.cases {
            count += 1;
            if let Err(e) = run(&case) {
                failures.push(format!("{fname} › {}: {e}", case.name));
            }
        }
    }
    assert!(count > 0, "no scenarios found");
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

fn run(case: &Case) -> Result<(), String> {
    let mut db = if case.start == "seed" {
        seed()
    } else {
        serde_json::from_value(case.start.clone()).map_err(|e| format!("start: {e}"))?
    };
    for (i, step) in case.steps.iter().enumerate() {
        let change: Change = serde_json::from_value(step.change.clone())
            .map_err(|e| format!("step {i}: bad change: {e}"))?;
        // Each step is one change from device "dev", one second apart.
        let stamp = Stamp {
            client: "dev".into(),
            seq: i as u64 + 1,
            at: 1000 * (i as i64 + 1),
        };
        let before = db.clone();
        let p = format!("step {i}");
        match (apply(&mut db, &stamp, &change), &step.rejected) {
            (Ok(a), None) => {
                if let Some(ok) = &step.ok {
                    subset(ok, &serde_json::to_value(&a).unwrap(), &p)?;
                }
                // Undo, where offered, really undoes it: applying it to the
                // result gives data that means the same as before.
                if let Some(undo) = &a.undo {
                    check_undo(&before, &db, &stamp, undo).map_err(|e| format!("{p} undo: {e}"))?;
                }
            }
            (Ok(a), Some(r)) => return Err(format!("{p}: applied ({a:?}), expected refusal {r}")),
            (Err(r), Some(er)) => {
                subset(er, &serde_json::to_value(&r).unwrap(), &p)?;
                if db != before {
                    return Err(format!("{p}: refused but changed the data"));
                }
            }
            (Err(r), None) => return Err(format!("{p}: refused: {r} ({r:?})")),
        }
    }
    let as_json = serde_json::to_value(&db).unwrap();
    if let Some(e) = &case.expected {
        subset(e, &as_json, "db")?;
    }
    if let Some(e) = &case.today {
        subset(e, &serde_json::to_value(today(&db)).unwrap(), "today")?;
    }
    if let Some(e) = &case.backlog {
        subset(
            e,
            &serde_json::to_value(open_backlog(&db)).unwrap(),
            "backlog",
        )?;
    }
    if let Some(e) = &case.deleted {
        subset(e, &serde_json::to_value(deleted(&db)).unwrap(), "deleted")?;
    }
    // Saved data loads back as the same.
    let again: Db = serde_json::from_value(as_json).map_err(|e| format!("reload: {e}"))?;
    if again != db {
        return Err("data changed after a save and load".into());
    }
    Ok(())
}

/// Undoing must restore what the user sees: the same live routines, runs,
/// steps, marks, placements and backlog. What it cannot restore exactly
/// does not show: the order marks were made in, the time a run or to-do
/// that finishes again finished, the options remembered for the next start.
fn check_undo(before: &Db, after: &Db, stamp: &Stamp, undo: &Change) -> Result<(), String> {
    let mut undone = after.clone();
    let s = Stamp {
        seq: stamp.seq + 1_000_000,
        ..stamp.clone()
    };
    apply(&mut undone, &s, undo).map_err(|r| format!("refused: {r}"))?;
    let view = |db: &Db| {
        let mut v = serde_json::to_value(db).unwrap();
        // A run created and then abandoned by its undo stays, removed;
        // compare live things only.
        strip_removed(&mut v);
        v
    };
    let (b, u) = (view(before), view(&undone));
    if b != u {
        return Err(format!("\nbefore: {b}\nundone: {u}"));
    }
    Ok(())
}

fn strip_removed(v: &mut Value) {
    for key in ["routines", "runs"] {
        if let Some(map) = v[key].as_object_mut() {
            map.retain(|_, x| x.get("removed").is_none());
            for x in map.values_mut() {
                if let Some(o) = x.as_object_mut() {
                    o.remove("lastFlags");
                    if let Some(f) = o.get_mut("finishedAt") {
                        *f = Value::Bool(true);
                    }
                    if let Some(Value::Object(done)) = o.get_mut("done") {
                        done.sort_keys();
                    }
                }
            }
        }
    }
    if let Some(items) = v["backlog"].as_array_mut() {
        items.retain(|x| x.get("removed").is_none());
        for x in items.iter_mut() {
            if let Some(d) = x.get_mut("doneAt") {
                *d = Value::Bool(true);
            }
        }
    }
}
