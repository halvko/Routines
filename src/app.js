import { expand, collectFlags, ready, waiting, isFinished, wouldCycle, includes } from './graph.js';
import { load, save } from './store.js';

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

/* ---------- views ---------- */

function homeView() {
  const runs = Object.values(db.runs).sort((a, b) => b.startedAt - a.startedAt);
  const active = runs.filter((r) => !r.finishedAt);
  const recent = runs.filter((r) => r.finishedAt).slice(0, 5);
  const routines = Object.values(db.routines);

  const runCard = (r) => {
    const total = r.nodes.length;
    const n = r.nodes.filter((x) => r.done[x.id]).length;
    const flags = Object.keys(r.flags).filter((f) => r.flags[f]);
    return `<a class="card" style="display:block;color:inherit;text-decoration:none" href="#/run/${r.id}">
      <div class="row"><strong class="grow">${esc(r.name)}</strong><span class="muted">${n}/${total}</span></div>
      ${flags.length ? `<div class="tags">${flags.map((f) => `<span class="tag">${esc(f)}</span>`).join('')}</div>` : ''}
      <div class="bar"><i style="width:${total ? (n / total) * 100 : 100}%"></i></div></a>`;
  };

  return `
    <h1>Routines</h1>
    ${active.length ? `<h2>In progress</h2>${active.map(runCard).join('')}` : ''}
    <h2>Start a routine</h2>
    ${routines.map((r) => `
      <div class="card row">
        <div class="grow"><strong>${esc(r.name)}</strong>
          <div class="muted">${r.steps.length} steps${collectFlags(db.routines, r.id).length ? ' · ' + collectFlags(db.routines, r.id).map(esc).join(', ') : ''}</div></div>
        <a class="btn" href="#/edit/${r.id}">Edit</a>
        <button class="primary" data-act="start" data-id="${r.id}">Start</button>
      </div>`).join('')}
    <button data-act="newRoutine">+ New routine</button>
    ${recent.length ? `<h2>Recently finished</h2>${recent.map((r) => `
      <div class="card row"><span class="grow">${esc(r.name)}</span>
        <span class="muted">${new Date(r.finishedAt).toLocaleDateString()}</span>
        <button class="ghost" data-act="delRun" data-id="${r.id}" aria-label="Remove">✕</button></div>`).join('')}` : ''}
    <h2>Data</h2>
    <div class="row"><button data-act="export">Export</button><button data-act="import">Import</button></div>`;
}

function runView(id) {
  const run = db.runs[id];
  if (!run) return `<p>Run not found. <a href="#/">Back</a></p>`;
  const names = Object.fromEntries(run.nodes.map((n) => [n.id, n.title]));
  const rdy = ready(run);
  const wait = waiting(run);
  const done = run.nodes.filter((n) => run.done[n.id]);
  const finished = isFinished(run);
  const flags = Object.keys(run.flags).filter((f) => run.flags[f]);
  const src = (n) => (n.source !== run.name ? `<div class="muted">${esc(n.source)}</div>` : '');

  return `
    <h1>${esc(run.name)}</h1>
    ${flags.length ? `<div class="tags" style="margin-bottom:8px">${flags.map((f) => `<span class="tag">${esc(f)}</span>`).join('')}</div>` : ''}
    <div class="bar"><i style="width:${(done.length / (run.nodes.length || 1)) * 100}%"></i></div>
    ${finished ? `<div class="banner">All done ✓</div><button class="primary" data-act="finish" data-id="${run.id}">Finish</button>` : ''}
    ${rdy.length ? `<h2>Do now</h2>${rdy.map((n) => `
      <div class="card step">
        <button class="check" data-act="toggle" data-run="${run.id}" data-id="${esc(n.id)}" aria-label="Done"></button>
        <div class="grow label">${esc(n.title)}${src(n)}</div>
        <button class="ghost" data-act="skip" data-run="${run.id}" data-id="${esc(n.id)}">Skip</button>
      </div>`).join('')}` : ''}
    ${wait.length ? `<h2>Waiting</h2>${wait.map((n) => `
      <div class="card step blocked">
        <span class="check"></span>
        <div class="grow label">${esc(n.title)}${src(n)}
          <div class="muted">after ${n.blockedBy.map((b) => esc(names[b])).join(', ')}</div></div>
      </div>`).join('')}` : ''}
    ${done.length ? `<h2>Done</h2>${done.map((n) => `
      <div class="card step ${run.done[n.id]}">
        <button class="check" data-act="toggle" data-run="${run.id}" data-id="${esc(n.id)}" aria-label="Undo">${run.done[n.id] === 'done' ? '✓' : '–'}</button>
        <div class="grow label">${esc(n.title)}</div>
      </div>`).join('')}` : ''}
    <div class="row" style="margin-top:20px"><a class="btn" href="#/">Back</a>
      <button class="danger" data-act="abandon" data-id="${run.id}">Abandon run</button></div>`;
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
    <div class="row" style="margin-top:24px"><a class="btn" href="#/">Done</a>
      <button class="danger" data-act="delRoutine">Delete routine</button></div>`;
}

/* ---------- routing / render ---------- */

function route() {
  const [, view, id] = location.hash.split('/');
  return { view: view || 'home', id };
}

function render() {
  const { view, id } = route();
  $app.innerHTML = view === 'run' ? runView(id) : view === 'edit' ? editView(id) : homeView();
}

window.addEventListener('hashchange', () => { render(); window.scrollTo(0, 0); });

/* ---------- runs ---------- */

function createRun(rid, flags) {
  const r = db.routines[rid];
  let nodes;
  try { nodes = expand(db.routines, rid, flags); } catch (e) { toast(e.message); return; }
  const run = { id: uid(), routineId: rid, name: r.name, flags, nodes, done: {}, startedAt: Date.now() };
  r.lastFlags = flags;
  db.runs[run.id] = run;
  persist();
  location.hash = `#/run/${run.id}`;
}

function startRoutine(rid) {
  const flagNames = collectFlags(db.routines, rid);
  if (!flagNames.length) return createRun(rid, {});
  const last = db.routines[rid].lastFlags || {};
  $dlg.innerHTML = `<form method="dialog">
    <h1>${esc(db.routines[rid].name)}</h1>
    <p class="muted">What's today like?</p>
    ${flagNames.map((f) => `<label class="flag"><input type="checkbox" name="${esc(f)}" ${last[f] ? 'checked' : ''}> ${esc(f)}</label>`).join('')}
    <div class="row" style="margin-top:12px"><button value="cancel">Cancel</button><button class="primary" value="go">Start</button></div></form>`;
  $dlg.onclose = () => {
    if ($dlg.returnValue !== 'go') return;
    const flags = {};
    $dlg.querySelectorAll('input[type=checkbox]').forEach((c) => { flags[c.name] = c.checked; });
    createRun(rid, flags);
  };
  $dlg.returnValue = '';
  $dlg.showModal();
}

/* ---------- actions ---------- */

const routineOf = () => db.routines[route().id];
const stepEl = (el) => routineOf().steps.find((s) => s.id === el.closest('[data-sid]').dataset.sid);

const actions = {
  start: (d) => startRoutine(d.id),
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
    location.hash = '#/';
  },
  toggle(d) {
    const run = db.runs[d.run];
    if (run.done[d.id]) delete run.done[d.id]; else run.done[d.id] = 'done';
    persist(); render();
  },
  skip(d) { db.runs[d.run].done[d.id] = 'skipped'; persist(); render(); },
  finish(d) { db.runs[d.id].finishedAt = Date.now(); persist(); location.hash = '#/'; },
  abandon(d) { if (confirm('Abandon this run?')) { delete db.runs[d.id]; persist(); location.hash = '#/'; } },
  delRun(d) { delete db.runs[d.id]; persist(); render(); },

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
  const el = e.target.closest('[data-act]');
  if (!el) return;
  actions[el.dataset.act]?.(el.dataset, el);
});

document.addEventListener('change', (e) => {
  const t = e.target;
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
    db = data;
    persist(); render();
    toast('Imported');
  } catch { toast('Not a valid routines file'); }
});

render();
