//! Where the core deliberately differs from `src/graph.js`. These cannot be
//! shared vectors, since the JavaScript gives other results. Delete this file
//! once the JavaScript rules are retired.

use indexmap::IndexMap;
use routines_core::*;
use serde_json::json;

/// In JavaScript, `flags[flag]` finds members every object inherits, so a
/// condition like "constructor" or "toString" holds even when the flag was
/// never chosen. In the core they are ordinary flags.
#[test]
fn inherited_object_member_names_are_ordinary_flags() {
    let none = Flags::default();
    for name in [
        "constructor",
        "toString",
        "__proto__",
        "hasOwnProperty",
        "valueOf",
    ] {
        assert!(
            !cond_holds(Some(name), &none),
            "{name} holds without being chosen"
        );
        let on: Flags = [(name.to_lowercase(), true)].into_iter().collect();
        assert!(
            cond_holds(Some(name), &on),
            "{name} does not hold when chosen"
        );
    }
}

/// In JavaScript, a routine id like "constructor" that is not in the map is
/// found on the object's prototype and crashes. In the core it is missing.
#[test]
fn inherited_object_member_names_are_ordinary_routine_ids() {
    let routines: IndexMap<String, Routine> = IndexMap::new();
    assert!(collect_flags(&routines, "constructor").is_empty());
    assert_eq!(
        expand(&routines, "constructor", &Flags::default()),
        Err(ExpandError::MissingRoutine)
    );

    let r: Routine = serde_json::from_value(json!({
        "id": "constructor", "name": "C",
        "steps": [{ "id": "a", "type": "task", "title": "A", "when": "toString", "after": [] }],
    }))
    .unwrap();
    let routines: IndexMap<String, Routine> =
        [("constructor".to_string(), r)].into_iter().collect();
    assert_eq!(collect_flags(&routines, "constructor"), ["tostring"]);
    assert!(
        expand(&routines, "constructor", &Flags::default())
            .unwrap()
            .is_empty()
    );
}
