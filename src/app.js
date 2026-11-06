import { expand, collectFlags, ready, waiting, isFinished, wouldCycle, includes, crossLinks, hasCycle } from './graph.js';
import { load, save, normalize } from './store.js';

let db = load();
const $app = document.getElementById('app');
const $dlg = document.getElementById('dlg');
const uid = () => Math.random().toString(36).slice(2, 8);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const persist = () => save(db);

function toast(msg) {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.classList.add('show');
  clearTimeout(toast.t);
  toast.t = setTimeout(() => el.classList.remove('show'), 2200);
}

/* ---------- dialogs ---------- */

// Opens a form dialog. onSubmit(formData, buttonValue) runs when closed via a submit button.
// Buttons with data-close (type=button) just close; Enter submits with the first submit button.
function openDialog(html, onSubmit) {
  $dlg.innerHTML = `<form method="dialog">${html}</form>`;
  const form = $dlg.querySelector('form');
  $dlg.onclose = () => {
    const v = $dlg.returnValue;
    if (v) onSubmit(new FormData(form), v);
  };
  $dlg.returnValue = '';
  $dlg.showModal();
  form.querySelector('[autofocus]')?.focus();
}

/* ---------- helpers ---------- */

const ctx = () => ({ runs: db.runs, links: crossLinks(db.runs) });
const activeRuns = () => Object.values(db.runs).filter((r) => !r.finishedAt).sort((a, b) => a.startedAt - b.startedAt);
const flagsOf = (run) => Object.keys(run.flags).filter((f) => run.flags[f]);
const progress = (run) => ({ n: run.nodes.filter((x) => run.done[x.id]).length, total: run.nodes.length });
const bar = ({ n, total }) => `<div class="bar"><i style="width:${total ? (n / total) * 100 : 100}%"></i></div>`;
const tags = (list) => list.map((f) => `<span class="tag">${esc(f)}</span>`).join('');
const isTodo = (run) => run.kind === 'todo';
const openBacklog = () => db.backlog.filter((i) => !i.doneAt);

function nodeLabel(x) {
  const r = db.runs[x.run];
  const node = r?.nodes.find((n) => n.id === x.id);
  if (!node) return '';
  return isTodo(r) || node.title === r.name ? node.title : `${node.title} (${r.name})`;
}

const placementText = (run) => (run.attach && db.runs[run.attach.run] ? `${run.attach.rel} ${nodeLabel(run.attach)}` : '');

// tag under a step: which routine it belongs to. In a single-routine context only sub-routines are named.
function tagFor(run, n, showRun) {
  const parts = [];
  if (showRun) parts.push(isTodo(run) ? 'To-do' : run.name);
  if (!isTodo(run) && n.source !== run.name) parts.push(n.source);
  return parts.length ? `<div class="from">${parts.map(esc).join(' · ')}</div>` : '';
}

const readyRow = (run, n, showRun) => `
  <div class="card step">
    <button class="check" data-act="toggle" data-run="${run.id}" data-id="${esc(n.id)}" aria-label="Done"></button>
    <div class="grow label">${esc(n.title)}${tagFor(run, n, showRun)}</div>
    <button class="ghost" data-act="skip" data-run="${run.id}" data-id="${esc(n.id)}">Skip</button>
  </div>`;

function waitingRow(run, n, showRun) {
  const names = Object.fromEntries(run.nodes.map((x) => [x.id, x.title]));
  const after = [...n.blockedBy.map((b) => esc(names[b])), ...n.blockedExt.map((x) => esc(nodeLabel(x)))];
  return `
  <div class="card step blocked">
    <span class="check"></span>
    <div class="grow label">${esc(n.title)}${tagFor(run, n, showRun)}
      <div class="muted">after ${after.join(', ')}</div></div>
  </div>`;
}

const runCard = (run) => `<a class="card runcard" href="#/run/${run.id}">
  <div class="row"><strong class="grow">${esc(run.name)}</strong>${tags(flagsOf(run))}
    <span class="muted">${progress(run).n}/${progress(run).total} ›</span></div>
  ${placementText(run) ? `<div class="placed">${esc(placementText(run))}</div>` : ''}
  ${isTodo(run) ? '' : bar(progress(run))}</a>`;

/* ---------- views ---------- */

function todayView() {
  const active = activeRuns();
  const c = ctx();
  const mode = db.prefs.todayMode;
  const startChips = `<div class="chips start">${Object.values(db.routines).map((r) =>
    `<button data-act="start" data-id="${r.id}">▶ ${esc(r.name)}</button>`).join('')}</div>`;

  if (!active.length) {
    return `<h1>Today</h1>
      <div class="card empty">Nothing running.<br>Start a routine, or pull something from the <a href="#/backlog">backlog</a>.</div>
      <h2>Start</h2>${startChips}`;
  }

  let body;
  if (mode === 'grouped') {
    body = active.map((run) => {
      const rdy = ready(run, c);
      const wait = waiting(run, c).length;
      return `<section class="group">
        <a class="group-head" href="#/run/${run.id}"><strong class="grow">${esc(run.name)}</strong>${tags(flagsOf(run))}
          <span class="muted">${progress(run).n}/${progress(run).total} ›</span></a>
        ${placementText(run) ? `<div class="placed">${esc(placementText(run))}</div>` : ''}
        ${isTodo(run) ? '' : bar(progress(run))}
        ${rdy.map((n) => readyRow(run, n, false)).join('')}
        ${wait ? `<div class="muted more">+ ${wait} waiting</div>` : ''}
      </section>`;
    }).join('');
  } else {
    const multi = active.length > 1;
    const rdy = active.flatMap((run) => ready(run, c).map((n) => readyRow(run, n, multi)));
    const wait = active.flatMap((run) => waiting(run, c).map((n) => waitingRow(run, n, multi)));
    body = `
      ${rdy.length ? `<h2>Do now</h2>${rdy.join('')}` : `<div class="card empty">Everything left is waiting on something.</div>`}
      ${wait.length ? `<details class="coming"><summary><h2>Coming up · ${wait.length}</h2></summary>${wait.join('')}</details>` : ''}
      <h2>Running</h2>${active.map(runCard).join('')}`;
  }

  return `
    <div class="row head"><h1 class="grow">Today</h1>
      <div class="seg" role="group" aria-label="Layout">
        <button class="${mode !== 'grouped' ? 'on' : ''}" data-act="mode" data-mode="mixed">Mixed</button>
        <button class="${mode === 'grouped' ? 'on' : ''}" data-act="mode" data-mode="grouped">By routine</button></div></div>
    ${body}
    <h2>Start</h2>${startChips}`;
}

function backlogView() {
  const items = openBacklog();
  const inToday = items.filter((i) => i.runId && db.runs[i.runId]);
  const waitingItems = items.filter((i) => !(i.runId && db.runs[i.runId]))
    .sort((a, b) => (b.starred ? 1 : 0) - (a.starred ? 1 : 0) || b.createdAt - a.createdAt);
  const done = db.backlog.filter((i) => i.doneAt).sort((a, b) => b.doneAt - a.doneAt).slice(0, 30);

  const row = (i) => `
    <div class="card step">
      <button class="check" data-act="itemDone" data-id="${i.id}" aria-label="Done"></button>
      <button class="grow label linkish" data-act="item" data-id="${i.id}">${esc(i.title)}</button>
      <button class="ghost star ${i.starred ? 'on' : ''}" data-act="star" data-id="${i.id}" aria-label="${i.starred ? 'Unstar' : 'Star'}">${i.starred ? '★' : '☆'}</button>
      <button class="ghost" data-act="pull" data-id="${i.id}">Today</button>
    </div>`;

  return `
    <h1>Backlog</h1>
    <form class="row capture" data-form="quickadd">
      <input type="text" name="title" placeholder="Dump something…" autocomplete="off" enterkeyhint="done">
      <button class="primary">Add</button>
    </form>
    ${inToday.length ? `<h2>In Today</h2>${inToday.map((i) => `
      <a class="card runcard" href="#/run/${i.runId}"><div class="row"><span class="grow">${esc(i.title)}</span>
        <span class="muted">${esc(placementText(db.runs[i.runId]) || 'now')} ›</span></div></a>`).join('')}` : ''}
    ${waitingItems.length ? `<h2>To do · ${waitingItems.length}</h2>${waitingItems.map(row).join('')}`
      : `<div class="card empty">Nothing here. Dump things as they come up — no dates, no plan.</div>`}
    ${done.length ? `<details class="coming"><summary><h2>Done · ${db.backlog.filter((i) => i.doneAt).length}</h2></summary>
      ${done.map((i) => `<div class="card step done">
        <button class="check" data-act="itemUndo" data-id="${i.id}" aria-label="Undo">✓</button>
        <div class="grow label">${esc(i.title)}</div></div>`).join('')}</details>` : ''}`;
}

function routinesView() {
  const recent = Object.values(db.runs).filter((r) => r.finishedAt && !isTodo(r)).sort((a, b) => b.finishedAt - a.finishedAt).slice(0, 5);
  return `
    <h1>Routines</h1>
    ${Object.values(db.routines).map((r) => {
      const fl = collectFlags(db.routines, r.id);
      return `<div class="card row">
        <div class="grow"><strong>${esc(r.name)}</strong>
          <div class="muted">${r.steps.length} steps${fl.length ? ' · ' + fl.map(esc).join(', ') : ''}</div></div>
        <a class="btn" href="#/edit/${r.id}">Edit</a>
        <button class="primary" data-act="start" data-id="${r.id}">Start</button>
      </div>`;
    }).join('')}
    <button data-act="newRoutine">+ New routine</button>
    ${recent.length ? `<h2>Recently finished</h2>${recent.map((r) => `
      <div class="card row"><a class="grow" href="#/run/${r.id}">${esc(r.name)}</a>
        <span class="muted">${new Date(r.finishedAt).toLocaleDateString()}</span>
        <button class="ghost" data-act="delRun" data-id="${r.id}" aria-label="Remove">✕</button></div>`).join('')}` : ''}
    <h2>Data</h2>
    <div class="row"><button data-act="export">Export</button><button data-act="import">Import</button></div>`;
}

function runView(id) {
  const run = db.runs[id];
  if (!run) return `<p>Not found. <a href="#/">Back</a></p>`;
  const c = ctx();
  const rdy = ready(run, c);
  const wait = waiting(run, c);
  const done = run.nodes.filter((n) => run.done[n.id]);
  const placed = placementText(run);

  return `
    <a href="#/" class="back">‹ Today</a>
    <h1>${esc(run.name)}</h1>
    ${flagsOf(run).length ? `<div class="tags" style="margin-bottom:8px">${tags(flagsOf(run))}</div>` : ''}
    ${run.finishedAt ? '' : `<div class="card row placement">
      <span class="grow">${placed ? `Placed <strong>${esc(placed)}</strong>` : '<span class="muted">Not placed — available right away</span>'}</span>
      <button data-act="place" data-id="${run.id}">${placed ? 'Move' : 'Place…'}</button>
      ${placed ? `<button class="ghost" data-act="detach" data-id="${run.id}" aria-label="Unplace">✕</button>` : ''}</div>`}
    ${isTodo(run) ? '' : bar(progress(run))}
    ${run.finishedAt ? `<div class="banner">All done ✓</div>` : ''}
    ${rdy.length ? `<h2>Do now</h2>${rdy.map((n) => readyRow(run, n, false)).join('')}` : ''}
    ${wait.length ? `<h2>Waiting</h2>${wait.map((n) => waitingRow(run, n, false)).join('')}` : ''}
    ${done.length ? `<h2>Done</h2>${done.map((n) => `
      <div class="card step ${run.done[n.id]}">
        <button class="check" data-act="toggle" data-run="${run.id}" data-id="${esc(n.id)}" aria-label="Undo">${run.done[n.id] === 'done' ? '✓' : '–'}</button>
        <div class="grow label">${esc(n.title)}</div>
      </div>`).join('')}` : ''}
    <div class="row" style="margin-top:20px">
      ${isTodo(run) && !run.finishedAt ? `<button data-act="unpull" data-id="${run.id}">Back to backlog</button>` : ''}
      <button class="danger" data-act="abandon" data-id="${run.id}">${isTodo(run) ? 'Delete to-do' : run.finishedAt ? 'Delete run' : 'Abandon run'}</button></div>`;
}

const stepLabel = (s) => (s.type === 'routine' ? '↳ ' + (db.routines[s.routineId]?.name ?? '(missing)') : s.title || '(untitled)');

function editView(id) {
  const r = db.routines[id];
  if (!r) return `<p>Routine not found. <a href="#/">Back</a></p>`;
  const flags = collectFlags(db.routines, id);
  const subOptions = Object.values(db.routines).filter((o) => o.id !== id && !includes(db.routines, o.id, id));

  return `
    <h1>Edit routine</h1>
    <div class="card"><input type="text" data-routine-name value="${esc(r.name)}" aria-label="Routine name">
      ${flags.length ? `<div class="muted" style="margin-top:8px">Options when starting: ${flags.map((f) => `<span class="tag">${esc(f)}</span>`).join(' ')}</div>` : ''}</div>
    <h2>Steps</h2>
    <p class="muted" style="margin-top:-4px">"After" decides what must be done first. Leave it empty and the step is available from the start. "Only if" shows the step only when that option is on (<code>gym</code>) or off (<code>!gym</code>). Steps that are hidden are skipped over automatically.</p>
    ${r.steps.map((s) => `
      <div class="card" data-sid="${s.id}">
        <div class="row">
          <div class="grow">
            ${s.type === 'routine'
              ? `<div class="badge">Sub-routine</div><select data-f="routineId">${subOptions.concat(db.routines[s.routineId] && !subOptions.includes(db.routines[s.routineId]) ? [db.routines[s.routineId]] : [])
                  .map((o) => `<option value="${o.id}" ${o.id === s.routineId ? 'selected' : ''}>${esc(o.name)}</option>`).join('')}</select>`
              : `<div class="badge">Step</div><input type="text" data-f="title" value="${esc(s.title)}" placeholder="What needs doing?">`}
          </div>
          <button class="ghost" data-act="moveStep" data-dir="-1" aria-label="Move up">↑</button>
          <button class="ghost" data-act="moveStep" data-dir="1" aria-label="Move down">↓</button>
          <button class="ghost danger" data-act="delStep" aria-label="Delete step">✕</button>
        </div>
        <label class="small">Only if <input type="text" data-f="when" value="${esc(s.when)}" placeholder="e.g. gym, !gym, abroad"></label>
        ${r.steps.length > 1 ? `<div class="chips"><span class="muted">After:</span>
          ${r.steps.filter((o) => o.id !== s.id).map((o) => `<button class="chip ${s.after.some((e) => e.id === o.id) ? 'on' : ''}" data-act="toggleDep" data-dep="${o.id}">${esc(stepLabel(o))}</button>`).join('')}</div>` : ''}
      </div>`).join('')}
    <div class="row">
      <button class="primary" data-act="addStep">+ Step</button>
      ${subOptions.length ? `<button data-act="addSub">+ Sub-routine</button>` : ''}
    </div>
    <div class="row" style="margin-top:24px"><a class="btn" href="#/routines">Done</a>
      <button class="danger" data-act="delRoutine">Delete routine</button></div>`;
}

/* ---------- routing / render ---------- */

function route() {
  const [, view, id] = location.hash.split('/');
  return { view: view || 'today', id };
}

function render() {
  const { view, id } = route();
  const views = { today: todayView, backlog: backlogView, routines: routinesView, run: () => runView(id), edit: () => editView(id) };
  $app.innerHTML = (views[view] || todayView)();
  const tab = view === 'edit' ? 'routines' : view === 'run' ? 'today' : view;
  document.querySelectorAll('[data-tab]').forEach((a) => a.classList.toggle('on', a.dataset.tab === tab));
  const open = openBacklog().filter((i) => !(i.runId && db.runs[i.runId])).length;
  document.getElementById('backlog-count').textContent = open || '';
}

window.addEventListener('hashchange', () => { render(); window.scrollTo(0, 0); });

/* ---------- runs ---------- */

function addRun(run) {
  db.runs[run.id] = run;
  persist();
}

function createRun(rid, flags) {
  const r = db.routines[rid];
  let nodes;
  try { nodes = expand(db.routines, rid, flags); } catch (e) { toast(e.message); return; }
  r.lastFlags = flags;
  addRun({ id: uid(), routineId: rid, name: r.name, flags, nodes, done: {}, startedAt: Date.now() });
  toast(`Started ${r.name}`);
  if (route().view === 'today') render(); else location.hash = '#/';
}

function startRoutine(rid) {
  const flagNames = collectFlags(db.routines, rid);
  if (!flagNames.length) return createRun(rid, {});
  const last = db.routines[rid].lastFlags || {};
  openDialog(`
    <h1>${esc(db.routines[rid].name)}</h1>
    <p class="muted">What's today like?</p>
    ${flagNames.map((f) => `<label class="flag"><input type="checkbox" name="${esc(f)}" ${last[f] ? 'checked' : ''}> ${esc(f)}</label>`).join('')}
    <div class="row end"><button type="button" data-close>Cancel</button><button class="primary" value="go">Start</button></div>`,
  (fd) => createRun(rid, Object.fromEntries(flagNames.map((f) => [f, fd.has(f)]))));
}

// A run finishes by itself when its last step is resolved, and reopens if you undo one.
function settle(run) {
  const fin = isFinished(run);
  if (fin && !run.finishedAt) { run.finishedAt = Date.now(); toast(`${run.name} done ✓`); }
  if (!fin) delete run.finishedAt;
  if (run.backlogId) {
    const item = db.backlog.find((i) => i.id === run.backlogId);
    if (item) item.doneAt = run.finishedAt || undefined;
  }
  persist(); render();
}

// Remove a run; anything placed relative to it becomes free again.
function removeRun(id) {
  Object.values(db.runs).forEach((r) => { if (r.attach?.run === id) delete r.attach; });
  delete db.runs[id];
}

/* ---------- placing ---------- */

// Ask where something should go: right away, or after/before a step of another running routine.
function placeDialog(title, excludeRunId, current, onPick) {
  const groups = activeRuns().filter((r) => r.id !== excludeRunId).map((r) => ({
    run: r,
    nodes: r.nodes.filter((n) => !r.done[n.id]),
  })).filter((g) => g.nodes.length);
  if (!groups.length) {
    if (current === undefined) return onPick(null);
    return toast('Start something first — there is nothing to place it next to.');
  }
  const cur = current ? `${current.run}|${current.id}` : '';
  openDialog(`
    <h1>${esc(title)}</h1>
    <div class="seg wide" role="radiogroup">
      <label><input type="radio" name="where" value="now" ${!current ? 'checked' : ''}><span>Right away</span></label>
      <label><input type="radio" name="where" value="after" ${current?.rel === 'after' ? 'checked' : ''}><span>After</span></label>
      <label><input type="radio" name="where" value="before" ${current?.rel === 'before' ? 'checked' : ''}><span>Before</span></label>
    </div>
    <select name="step" aria-label="Step">
      ${groups.map((g) => `<optgroup label="${esc(g.run.name)}">${g.nodes.map((n) =>
        `<option value="${g.run.id}|${esc(n.id)}" ${cur === `${g.run.id}|${n.id}` ? 'selected' : ''}>${esc(n.title)}</option>`).join('')}</optgroup>`).join('')}
    </select>
    <p class="muted small-print">"After" keeps it waiting until that step is done. "Before" holds that step back until this is done.</p>
    <div class="row end"><button type="button" data-close>Cancel</button><button class="primary" value="ok">OK</button></div>`,
  (fd) => {
    const rel = fd.get('where');
    if (rel === 'now') return onPick(null);
    const [run, ...rest] = fd.get('step').split('|');
    onPick({ rel, run, id: rest.join('|') });
  });
  // the step list only matters when after/before is chosen
  const form = $dlg.querySelector('form');
  const sync = () => { form.elements.namedItem('step').disabled = form.elements.namedItem('where').value === 'now'; };
  form.addEventListener('change', sync);
  sync();
}

function applyPlacement(run, p) {
  const prev = run.attach;
  if (p) run.attach = p; else delete run.attach;
  if (hasCycle(db.runs)) {
    if (prev) run.attach = prev; else delete run.attach;
    toast('That would make things wait on each other in a loop');
    return false;
  }
  return true;
}

/* ---------- backlog ---------- */

const item = (id) => db.backlog.find((i) => i.id === id);

function addItem(title, starred = false) {
  title = title.trim();
  if (!title) return;
  db.backlog.push({ id: uid(), title, starred, createdAt: Date.now() });
  persist();
  render();
  toast('Added to backlog');
}

function pullItem(it) {
  placeDialog(`Do “${it.title}”`, null, undefined, (p) => {
    const run = { id: uid(), kind: 'todo', backlogId: it.id, name: it.title, flags: {}, done: {}, startedAt: Date.now(),
      nodes: [{ id: 'task', title: it.title, deps: [], source: it.title }] };
    addRun(run);
    if (p && !applyPlacement(run, p)) { removeRun(run.id); persist(); return; }
    it.runId = run.id;
    persist();
    toast(p ? `In Today, ${p.rel} ${nodeLabel(p)}` : 'In Today');
    render();
  });
}

function captureDialog() {
  openDialog(`
    <h1>Add to backlog</h1>
    <input type="text" name="title" placeholder="What needs doing?" autocomplete="off" autofocus enterkeyhint="done">
    <label class="flag"><input type="checkbox" name="starred"> ★ Important</label>
    <div class="row end"><button type="button" data-close>Cancel</button><button class="primary" value="add">Add</button></div>`,
  (fd) => addItem(fd.get('title') || '', fd.has('starred')));
}

function itemDialog(it) {
  openDialog(`
    <h1>To-do</h1>
    <input type="text" name="title" value="${esc(it.title)}" autocomplete="off">
    <label class="flag"><input type="checkbox" name="starred" ${it.starred ? 'checked' : ''}> ★ Important</label>
    <div class="row end rev">
      <button class="primary" value="save">Save</button><button value="pull">Do today…</button>
      <span class="grow"></span><button class="danger" value="delete">Delete</button></div>`,
  (fd, v) => {
    if (v === 'delete') {
      if (!confirm(`Delete “${it.title}”?`)) return;
      db.backlog = db.backlog.filter((i) => i !== it);
      persist(); render(); return;
    }
    it.title = (fd.get('title') || '').trim() || it.title;
    it.starred = fd.has('starred');
    persist(); render();
    if (v === 'pull') pullItem(it);
  });
}

/* ---------- actions ---------- */

const routineOf = () => db.routines[route().id];
const stepEl = (el) => routineOf().steps.find((s) => s.id === el.closest('[data-sid]').dataset.sid);

const actions = {
  start: (d) => startRoutine(d.id),
  mode(d) { db.prefs.todayMode = d.mode; persist(); render(); },
  toggle(d) {
    const run = db.runs[d.run];
    if (run.done[d.id]) delete run.done[d.id]; else run.done[d.id] = 'done';
    settle(run);
  },
  skip(d) { db.runs[d.run].done[d.id] = 'skipped'; settle(db.runs[d.run]); },
  abandon(d) {
    const run = db.runs[d.id];
    if (!confirm(isTodo(run) ? `Delete “${run.name}” completely?` : 'Remove this run?')) return;
    if (run.backlogId) db.backlog = db.backlog.filter((i) => i.id !== run.backlogId);
    removeRun(d.id); persist(); location.hash = '#/';
  },
  unpull(d) {
    const run = db.runs[d.id];
    const it = item(run.backlogId);
    if (it) delete it.runId;
    removeRun(d.id); persist();
    toast('Back in the backlog');
    location.hash = '#/backlog';
  },
  delRun(d) { removeRun(d.id); persist(); render(); },
  place(d) {
    const run = db.runs[d.id];
    placeDialog(`Place “${run.name}”`, run.id, run.attach || null, (p) => { if (applyPlacement(run, p)) { persist(); render(); } });
  },
  detach(d) { delete db.runs[d.id].attach; persist(); render(); },

  capture: () => captureDialog(),
  item: (d) => itemDialog(item(d.id)),
  pull: (d) => pullItem(item(d.id)),
  star(d) { const it = item(d.id); it.starred = !it.starred; persist(); render(); },
  itemDone(d) { item(d.id).doneAt = Date.now(); persist(); render(); toast('Done ✓'); },
  itemUndo(d) {
    const it = item(d.id);
    delete it.doneAt;
    // if it was done via Today, reopen that run too
    const run = it.runId && db.runs[it.runId];
    if (run) { run.done = {}; delete run.finishedAt; }
    persist(); render();
  },

  newRoutine() {
    const id = uid();
    db.routines[id] = { id, name: 'New routine', steps: [] };
    persist();
    location.hash = `#/edit/${id}`;
  },
  delRoutine() {
    const r = routineOf();
    const users = Object.values(db.routines).filter((o) => o.id !== r.id && o.steps.some((s) => s.type === 'routine' && s.routineId === r.id));
    if (users.length) return toast(`Used by ${users.map((u) => u.name).join(', ')}`);
    if (!confirm(`Delete "${r.name}"?`)) return;
    delete db.routines[r.id];
    persist();
    location.hash = '#/routines';
  },
  addStep() {
    const r = routineOf();
    const prev = r.steps[r.steps.length - 1];
    r.steps.push({ id: uid(), type: 'task', title: '', when: '', after: prev ? [{ id: prev.id }] : [] });
    persist(); render();
    $app.querySelector('[data-sid]:last-of-type input[data-f=title]')?.focus();
  },
  addSub() {
    const r = routineOf();
    const opt = Object.values(db.routines).find((o) => o.id !== r.id && !includes(db.routines, o.id, r.id));
    const prev = r.steps[r.steps.length - 1];
    r.steps.push({ id: uid(), type: 'routine', routineId: opt.id, when: '', after: prev ? [{ id: prev.id }] : [] });
    persist(); render();
  },
  delStep(_, el) {
    const r = routineOf();
    const s = stepEl(el);
    // reconnect: whatever waited on this step now waits on what it waited on
    r.steps.forEach((o) => {
      if (o.after.some((e) => e.id === s.id)) {
        o.after = o.after.filter((e) => e.id !== s.id);
        s.after.forEach((e) => { if (!o.after.some((x) => x.id === e.id) && !wouldCycle(r, o.id, e.id)) o.after.push({ ...e }); });
      }
    });
    r.steps = r.steps.filter((o) => o !== s);
    persist(); render();
  },
  moveStep(d, el) {
    const r = routineOf();
    const i = r.steps.indexOf(stepEl(el));
    const j = i + Number(d.dir);
    if (j < 0 || j >= r.steps.length) return;
    [r.steps[i], r.steps[j]] = [r.steps[j], r.steps[i]];
    persist(); render();
  },
  toggleDep(d, el) {
    const r = routineOf();
    const s = stepEl(el);
    const has = s.after.some((e) => e.id === d.dep);
    if (has) s.after = s.after.filter((e) => e.id !== d.dep);
    else if (wouldCycle(r, s.id, d.dep)) return toast("That would make a loop — it already waits on this step");
    else s.after.push({ id: d.dep });
    persist(); render();
  },

  export() {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([JSON.stringify(db, null, 2)], { type: 'application/json' }));
    a.download = 'routines.json';
    a.click();
  },
  import: () => document.getElementById('file').click(),
};

document.addEventListener('click', (e) => {
  if (e.target.closest('[data-close]')) { $dlg.close(); return; }
  const el = e.target.closest('[data-act]');
  if (!el) return;
  actions[el.dataset.act]?.(el.dataset, el);
});

document.addEventListener('submit', (e) => {
  const f = e.target;
  if (f.dataset.form !== 'quickadd') return;
  e.preventDefault();
  addItem(f.title.value);
  $app.querySelector('[data-form=quickadd] input')?.focus();
});

document.addEventListener('change', (e) => {
  const t = e.target;
  if (t.closest('dialog')) return;
  if (t.matches('[data-routine-name]')) {
    routineOf().name = t.value.trim() || 'Untitled';
    persist(); render();
  } else if (t.dataset.f) {
    const s = stepEl(t);
    s[t.dataset.f] = t.value;
    persist();
    render();
  }
});

document.getElementById('file').addEventListener('change', async (e) => {
  const f = e.target.files[0];
  e.target.value = '';
  if (!f) return;
  try {
    const data = JSON.parse(await f.text());
    if (!data.routines || !data.runs) throw new Error();
    if (!confirm('Replace everything with the imported file?')) return;
    db = normalize(data);
    persist(); render();
    toast('Imported');
  } catch { toast('Not a valid routines file'); }
});

render();

if ('serviceWorker' in navigator) navigator.serviceWorker.register('sw.js').catch(() => {});
// Ask the browser not to evict our localStorage under storage pressure.
navigator.storage?.persist?.().catch(() => {});
