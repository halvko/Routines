//! Two devices and the server, syncing through the outbox and the log
//! (decision 0003): offline changes on both sides, retries, gaps, changes
//! the server refuses, and Claude making changes through MCP.

use routines_core::*;
use serde_json::json;

/// The server's log, with the entries it stored (the shell's job).
#[derive(Default)]
struct Server {
    log: Log,
    entries: Vec<LogEntry>,
}

impl Server {
    fn push(&mut self, changes: Vec<Envelope>) {
        for c in changes {
            if let Some(e) = self.log.push(c) {
                self.entries.push(e);
            }
        }
    }

    fn after(&self, n: u64) -> Vec<LogEntry> {
        self.entries.iter().filter(|e| e.n > n).cloned().collect()
    }
}

fn change(v: serde_json::Value) -> Change {
    serde_json::from_value(v).unwrap()
}

/// A device: its client, plus whether it is online.
struct Device {
    client: Client,
    online: bool,
    shown: Vec<Effect>,
}

impl Device {
    fn new(id: &str) -> Self {
        Device {
            client: Client::new(id),
            online: true,
            shown: Vec::new(),
        }
    }

    /// Feed an event and carry out the effects the way a shell would.
    fn handle(&mut self, server: &mut Server, event: Event) {
        let mut queue = vec![event];
        while let Some(ev) = queue.pop() {
            for fx in self.client.update(ev) {
                match fx {
                    Effect::Push { changes } if self.online => server.push(changes),
                    Effect::Pull { after } if self.online => queue.push(Event::Pulled {
                        entries: server.after(after),
                    }),
                    Effect::Push { .. } | Effect::Pull { .. } | Effect::Save => {}
                    other => self.shown.push(other),
                }
            }
        }
    }

    fn act(&mut self, server: &mut Server, at: i64, c: serde_json::Value) {
        self.handle(
            server,
            Event::Change {
                at,
                change: change(c),
            },
        );
    }

    fn sync(&mut self, server: &mut Server) {
        self.online = true;
        self.handle(server, Event::Connected);
    }
}

#[test]
fn a_change_on_one_device_shows_on_the_other() {
    let mut s = Server::default();
    let (mut phone, mut laptop) = (Device::new("phone"), Device::new("laptop"));
    phone.act(
        &mut s,
        1,
        json!({"op": "startRun", "run": "b", "routine": "breakfast"}),
    );
    phone.act(
        &mut s,
        2,
        json!({"op": "markStep", "run": "b", "step": "coffee", "mark": "done"}),
    );
    laptop.sync(&mut s);
    assert_eq!(laptop.client.state(), &s.log.db);
    assert_eq!(
        laptop.client.state().runs["b"].done.get("coffee"),
        Some(&Mark::Done)
    );
    phone.sync(&mut s);
    assert_eq!(phone.client.state(), &s.log.db);
    assert!(
        phone.client.saved().outbox.is_empty(),
        "the server took the phone's changes"
    );
}

#[test]
fn offline_changes_on_both_devices_both_survive() {
    let mut s = Server::default();
    let (mut phone, mut laptop) = (Device::new("phone"), Device::new("laptop"));
    phone.act(
        &mut s,
        1,
        json!({"op": "startRun", "run": "b", "routine": "breakfast"}),
    );
    laptop.sync(&mut s);
    phone.online = false;
    laptop.online = false;
    // Offline: the phone ticks a step and stars nothing; the laptop renames
    // the routine and adds a to-do.
    phone.act(
        &mut s,
        10,
        json!({"op": "markStep", "run": "b", "step": "coffee", "mark": "done"}),
    );
    laptop.act(
        &mut s,
        11,
        json!({"op": "renameRoutine", "routine": "breakfast", "name": "Brunch"}),
    );
    laptop.act(
        &mut s,
        12,
        json!({"op": "addItem", "item": "milk", "title": "Buy milk"}),
    );
    assert_eq!(
        s.entries.len(),
        1,
        "nothing reached the server while offline"
    );
    laptop.sync(&mut s);
    phone.sync(&mut s);
    laptop.sync(&mut s);
    for d in [&phone, &laptop] {
        let db = d.client.state();
        assert_eq!(
            db, &s.log.db,
            "every device ends up with the server's state"
        );
        assert_eq!(db.runs["b"].done.get("coffee"), Some(&Mark::Done));
        assert_eq!(db.routines["breakfast"].name, "Brunch");
        assert_eq!(db.backlog[0].title, "Buy milk");
    }
}

#[test]
fn edits_to_different_fields_both_survive_and_the_same_field_takes_the_later_one() {
    let mut s = Server::default();
    let (mut phone, mut laptop) = (Device::new("phone"), Device::new("laptop"));
    phone.online = false;
    laptop.online = false;
    phone.act(
        &mut s,
        1,
        json!({"op": "setStepTitle", "routine": "breakfast", "step": "coffee", "title": "Tea"}),
    );
    laptop.act(
        &mut s,
        2,
        json!({"op": "setStepWhen", "routine": "breakfast", "step": "coffee", "when": "home"}),
    );
    laptop.act(
        &mut s,
        3,
        json!({"op": "renameRoutine", "routine": "breakfast", "name": "Laptop"}),
    );
    phone.act(
        &mut s,
        4,
        json!({"op": "renameRoutine", "routine": "breakfast", "name": "Phone"}),
    );
    phone.sync(&mut s);
    laptop.sync(&mut s);
    phone.sync(&mut s);
    let r = &phone.client.state().routines["breakfast"];
    assert_eq!(r.steps[0].title.as_deref(), Some("Tea"));
    assert_eq!(r.steps[0].when.as_deref(), Some("home"));
    // The server orders the changes; the one it took last wins.
    assert_eq!(r.name, "Laptop");
    assert_eq!(laptop.client.state(), phone.client.state());
}

#[test]
fn a_change_that_no_longer_fits_is_kept_as_a_notice_never_dropped() {
    let mut s = Server::default();
    let (mut phone, mut laptop) = (Device::new("phone"), Device::new("laptop"));
    phone.act(
        &mut s,
        1,
        json!({"op": "startRun", "run": "b", "routine": "breakfast"}),
    );
    laptop.sync(&mut s);
    phone.online = false;
    laptop.act(&mut s, 2, json!({"op": "abandonRun", "run": "b"}));
    // Offline, the phone ticks a step of the run the laptop abandoned.
    phone.act(
        &mut s,
        3,
        json!({"op": "markStep", "run": "b", "step": "coffee", "mark": "done"}),
    );
    assert_eq!(
        phone.client.state().runs["b"].done.len(),
        1,
        "applied locally at once"
    );
    phone.sync(&mut s);
    let last = s.entries.last().unwrap();
    assert_eq!(
        last.rejected,
        Some(Rejected::Removed {
            kind: Kind::Run,
            id: "b".into()
        }),
        "the server logs the refused change with its reason"
    );
    assert_eq!(phone.client.notices().len(), 1);
    assert!(phone.shown.iter().any(|e| matches!(e, Effect::Notice(_))));
    assert!(phone.client.saved().outbox.is_empty());
    assert!(phone.client.state().runs["b"].removed.is_some());
    // The laptop sees the notice too; dismissing it is per device.
    laptop.sync(&mut s);
    assert_eq!(laptop.client.notices().len(), 1);
    let n = laptop.client.notices()[0].n;
    laptop.handle(&mut s, Event::Dismiss { n });
    assert!(laptop.client.notices().is_empty());
}

#[test]
fn a_refused_local_change_changes_nothing_and_is_not_sent() {
    let mut s = Server::default();
    let mut phone = Device::new("phone");
    phone.act(&mut s, 1, json!({"op": "addItem", "title": "  "}));
    assert_eq!(phone.shown, vec![Effect::Refused(Rejected::EmptyTitle)]);
    assert!(s.entries.is_empty());
    assert_eq!(
        phone.client.saved().seq,
        0,
        "a refused change uses no number"
    );
}

#[test]
fn pushing_the_same_changes_twice_is_safe() {
    let mut s = Server::default();
    let mut phone = Device::new("phone");
    phone.online = false;
    phone.act(
        &mut s,
        1,
        json!({"op": "addItem", "item": "a", "title": "A"}),
    );
    phone.act(
        &mut s,
        2,
        json!({"op": "starItem", "item": "a", "starred": true}),
    );
    let outbox = phone.client.saved().outbox.clone();
    s.push(outbox.clone());
    s.push(outbox);
    assert_eq!(s.entries.len(), 2);
    phone.sync(&mut s);
    assert_eq!(s.entries.len(), 2);
    assert_eq!(phone.client.state(), &s.log.db);
}

#[test]
fn a_stale_or_gappy_pull_is_handled() {
    let mut s = Server::default();
    let (mut phone, mut laptop) = (Device::new("phone"), Device::new("laptop"));
    for i in 0..3 {
        phone.act(
            &mut s,
            i,
            json!({"op": "addItem", "title": format!("T{i}")}),
        );
    }
    // An old reply arriving late changes nothing.
    laptop.sync(&mut s);
    let before = laptop.client.state().clone();
    let fx = laptop.client.update(Event::Pulled {
        entries: s.after(0),
    });
    assert!(fx.is_empty());
    assert_eq!(laptop.client.state(), &before);
    // Entries with a gap before them are not applied; the device asks again.
    let mut fresh = Client::new("tablet");
    let fx = fresh.update(Event::Pulled {
        entries: s.after(1),
    });
    assert_eq!(fx, vec![Effect::Pull { after: 0 }]);
    assert_eq!(fresh.saved().base_n, 0);
}

#[test]
fn a_run_started_offline_keeps_its_snapshot_even_if_the_routine_changed_meanwhile() {
    let mut s = Server::default();
    let (mut phone, mut laptop) = (Device::new("phone"), Device::new("laptop"));
    phone.online = false;
    phone.act(
        &mut s,
        1,
        json!({"op": "startRun", "run": "b", "routine": "breakfast"}),
    );
    laptop.act(
        &mut s,
        2,
        json!({"op": "setStepTitle", "routine": "breakfast", "step": "coffee", "title": "Tea"}),
    );
    phone.sync(&mut s);
    let run = &s.log.db.runs["b"];
    assert_eq!(
        run.nodes[0].title, "Start the coffee",
        "the phone's snapshot, not a new one"
    );
    assert_eq!(phone.client.state(), &s.log.db);
}

#[test]
fn a_device_carries_on_from_what_it_saved() {
    let mut s = Server::default();
    let mut phone = Device::new("phone");
    phone.online = false;
    phone.act(
        &mut s,
        1,
        json!({"op": "addItem", "item": "a", "title": "A"}),
    );
    let saved: Saved =
        serde_json::from_value(serde_json::to_value(phone.client.saved()).unwrap()).unwrap();
    let mut again = Device {
        client: Client::restore(saved),
        online: true,
        shown: Vec::new(),
    };
    assert_eq!(again.client.state(), phone.client.state());
    again.sync(&mut s);
    assert_eq!(s.log.db.backlog[0].id, "a");
}

#[test]
fn claude_through_mcp_is_one_more_client() {
    let mut s = Server::default();
    let mut phone = Device::new("phone");
    phone.act(
        &mut s,
        1,
        json!({"op": "startRun", "run": "m", "routine": "morning", "flags": {"gym": false}}),
    );
    // What Claude reads, and a call it makes, as the MCP server would handle
    // them: the call becomes a change that goes through the user's log.
    let today = serde_json::to_value(today(&s.log.db)).unwrap();
    assert_eq!(today["doNow"][0]["title"], "Get out of bed");
    let calls = [
        (
            "add_item",
            json!({"title": "Book flights", "starred": true}),
        ),
        (
            "mark_step",
            json!({"run": "m", "step": "wake", "mark": "done"}),
        ),
    ];
    for (seq, (tool, args)) in calls.into_iter().enumerate() {
        let c = change_from_tool(tool, args).unwrap();
        s.push(vec![Envelope {
            client: "mcp".into(),
            seq: seq as u64 + 1,
            at: 100,
            change: c,
        }]);
    }
    phone.sync(&mut s);
    let db = phone.client.state();
    assert_eq!(db.backlog[0].title, "Book flights");
    assert_eq!(
        db.backlog[0].id, "mcp-1",
        "ids Claude leaves out are made up for it"
    );
    assert_eq!(db.runs["m"].done.get("wake"), Some(&Mark::Done));
}
