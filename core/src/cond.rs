//! Conditions on flags: `"gym"`, `"!gym"`, `"abroad flying"`.

use crate::types::{Flags, Routine};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// One term of a condition: the flag must be on, or off when `neg`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    pub flag: String,
    pub neg: bool,
}

/// Whitespace as JavaScript's `\s` matches it.
fn is_js_space(c: char) -> bool {
    const OTHER: &[char] = &[
        ' ', '\u{A0}', '\u{1680}', '\u{2028}', '\u{2029}', '\u{202F}', '\u{205F}', '\u{3000}',
        '\u{FEFF}',
    ];
    matches!(c, '\t'..='\r' | '\u{2000}'..='\u{200A}') || OTHER.contains(&c)
}

/// Split a condition into terms. Terms are separated by whitespace or commas,
/// a leading `!` negates, and flag names are lowercased. All terms must hold.
pub fn parse_cond(cond: Option<&str>) -> Vec<Term> {
    cond.unwrap_or("")
        .split(|c: char| c == ',' || is_js_space(c))
        .filter(|t| !t.is_empty())
        .filter_map(|t| {
            let (flag, neg) = match t.strip_prefix('!') {
                Some(rest) => (rest, true),
                None => (t, false),
            };
            (!flag.is_empty()).then(|| Term {
                flag: flag.to_lowercase(),
                neg,
            })
        })
        .collect()
}

/// Does the condition hold for these flags? An empty condition always does.
pub fn cond_holds(cond: Option<&str>, flags: &Flags) -> bool {
    parse_cond(cond)
        .iter()
        .all(|t| flags.get(&t.flag).copied().unwrap_or(false) != t.neg)
}

/// All flag names used by a routine and the routines it includes, sorted.
pub fn collect_flags(routines: &IndexMap<String, Routine>, root_id: &str) -> Vec<String> {
    let mut found = IndexSet::new();
    let mut seen = IndexSet::new();
    visit(routines, root_id, &mut found, &mut seen);
    let mut flags: Vec<String> = found.into_iter().collect();
    // JavaScript sorts strings by UTF-16 code units.
    flags.sort_by(|a, b| utf16_cmp(a, b));
    flags
}

fn visit<'a>(
    routines: &'a IndexMap<String, Routine>,
    id: &'a str,
    found: &mut IndexSet<String>,
    seen: &mut IndexSet<&'a str>,
) {
    let Some(routine) = routines.get(id) else {
        return;
    };
    if !seen.insert(id) {
        return;
    }
    for s in &routine.steps {
        found.extend(parse_cond(s.when.as_deref()).into_iter().map(|t| t.flag));
        for e in &s.after {
            found.extend(parse_cond(e.when.as_deref()).into_iter().map(|t| t.flag));
        }
        if s.step_type == crate::StepType::Routine
            && let Some(sub) = &s.routine_id
        {
            visit(routines, sub, found, seen);
        }
    }
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
