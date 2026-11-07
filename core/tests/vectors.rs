//! Runs the shared test vectors in `tests/vectors/*.json`. Their expected
//! results come from the web app's `src/graph.js` (`npm run vectors`), and
//! `test/vectors.test.js` runs the same files against it.

use indexmap::IndexMap;
use routines_core::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct File {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    #[serde(rename = "fn")]
    func: String,
    input: Value,
    expected: Value,
}

#[derive(Deserialize)]
struct CondIn {
    cond: Option<String>,
    #[serde(default)]
    flags: Flags,
}

#[derive(Deserialize)]
struct RoutinesIn {
    routines: IndexMap<String, Routine>,
    root: String,
    #[serde(default)]
    flags: Flags,
}

#[derive(Deserialize)]
struct RunIn {
    run: Run,
}

/// `run` names one of `runs`; `placed` lets ready/waiting see placements.
#[derive(Deserialize)]
struct RunsIn {
    runs: Runs,
    run: Option<String>,
    #[serde(default)]
    placed: bool,
}

impl RunsIn {
    fn run(&self) -> &Run {
        &self.runs[self.run.as_deref().expect("case names a run")]
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WouldCycleIn {
    routine: Routine,
    step_id: String,
    dep_id: String,
}

#[derive(Deserialize)]
struct IncludesIn {
    routines: IndexMap<String, Routine>,
    rid: String,
    target: String,
}

fn to<T: Serialize>(x: &T) -> Value {
    serde_json::to_value(x).expect("serializable")
}

fn input<T: DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).expect("input matches the stored data types")
}

fn with_ctx<T>(i: &RunsIn, f: impl FnOnce(Option<&Ctx<'_>>) -> T) -> T {
    if i.placed {
        f(Some(&Ctx::new(&i.runs)))
    } else {
        f(None)
    }
}

/// What the core returns for one case, as JSON.
fn call(func: &str, v: &Value) -> Value {
    match func {
        "parseCond" => to(&parse_cond(input::<CondIn>(v).cond.as_deref())),
        "condHolds" => {
            let i: CondIn = input(v);
            json!(cond_holds(i.cond.as_deref(), &i.flags))
        }
        "collectFlags" => {
            let i: RoutinesIn = input(v);
            json!(collect_flags(&i.routines, &i.root))
        }
        "expand" => {
            let i: RoutinesIn = input(v);
            match expand(&i.routines, &i.root, &i.flags) {
                Ok(nodes) => to(&nodes),
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "roots" => to(&roots(&input::<RunIn>(v).run)),
        "sinks" => to(&sinks(&input::<RunIn>(v).run)),
        "isFinished" => json!(is_finished(&input::<RunIn>(v).run)),
        "ready" => {
            let i: RunsIn = input(v);
            with_ctx(&i, |ctx| to(&ready(i.run(), ctx)))
        }
        "waiting" => {
            let i: RunsIn = input(v);
            with_ctx(&i, |ctx| to(&waiting(i.run(), ctx)))
        }
        "crossLinks" => {
            let i: RunsIn = input(v);
            let links: Vec<Value> = cross_links(&i.runs)
                .into_iter()
                .map(|((run, id), from)| json!({ "run": run, "id": id, "from": from }))
                .collect();
            json!(links)
        }
        "hasCycle" => json!(has_cycle(&input::<RunsIn>(v).runs)),
        "wouldCycle" => {
            let i: WouldCycleIn = input(v);
            json!(would_cycle(&i.routine, &i.step_id, &i.dep_id))
        }
        "includes" => {
            let i: IncludesIn = input(v);
            json!(includes(&i.routines, &i.rid, &i.target))
        }
        other => panic!("unknown function {other:?}"),
    }
}

#[test]
fn vectors() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut failures = Vec::new();
    let mut count = 0;
    for path in &paths {
        let file: File = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        for c in file.cases {
            count += 1;
            let got = call(&c.func, &c.input);
            if got != c.expected {
                failures.push(format!(
                    "{name}: {}\n  expected: {}\n  got:      {}",
                    c.name, c.expected, got
                ));
            }
        }
    }
    assert!(count > 100, "only {count} cases found in {}", dir.display());
    assert!(
        failures.is_empty(),
        "{} of {count} cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
