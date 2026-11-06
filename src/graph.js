// Pure routine-graph logic: no DOM, no storage. Unit-tested in test/.
//
// Routine: { id, name, steps: Step[] }
// Step:    { id, type: 'task'|'routine', title?, routineId?, when?: string,
//            after: { id, when?: string }[] }
// `when` is a condition on flags: space/comma separated, "!" negates, all must hold.
//   e.g. "gym", "!gym", "abroad flying"

export function parseCond(cond) {
  return (cond || '')
    .split(/[\s,]+/)
    .filter(Boolean)
    .map((t) => (t.startsWith('!') ? { flag: t.slice(1).toLowerCase(), neg: true } : { flag: t.toLowerCase(), neg: false }))
    .filter((c) => c.flag);
}

export function condHolds(cond, flags) {
  return parseCond(cond).every(({ flag, neg }) => Boolean(flags[flag]) !== neg);
}

/** All flag names used by a routine and the routines it includes. */
export function collectFlags(routines, rootId) {
  const found = new Set();
  const seen = new Set();
  const visit = (id) => {
    if (seen.has(id) || !routines[id]) return;
    seen.add(id);
    for (const s of routines[id].steps) {
      parseCond(s.when).forEach((c) => found.add(c.flag));
      (s.after || []).forEach((e) => parseCond(e.when).forEach((c) => found.add(c.flag)));
      if (s.type === 'routine') visit(s.routineId);
    }
  };
  visit(rootId);
  return [...found].sort();
}

/**
 * Flatten a routine (with sub-routines) into concrete nodes for the given flags.
 * Steps whose condition fails are removed and their dependencies "bridged":
 * anything that waited on a removed step waits on what the removed step waited on.
 * Returns nodes in a valid topological order: { id, title, deps: string[], source }.
 */
export function expand(routines, rootId, flags = {}) {
  const nodes = [];

  function walk(routineId, prefix, entryDeps, stack) {
    const routine = routines[routineId];
    if (!routine) throw new Error('A sub-routine no longer exists');
    if (stack.includes(routineId)) throw new Error(`"${routine.name}" includes itself`);
    const byId = new Map(routine.steps.map((s) => [s.id, s]));
    const memo = new Map();
    const visiting = new Set();
    const start = nodes.length;

    // ids that anything depending on `step` must wait for
    const exitOf = (step) => {
      if (memo.has(step.id)) return memo.get(step.id);
      if (visiting.has(step.id)) throw new Error(`Circular dependency in "${routine.name}"`);
      visiting.add(step.id);

      const edges = (step.after || []).filter((e) => byId.has(e.id) && condHolds(e.when, flags));
      const deps = edges.length
        ? [...new Set(edges.flatMap((e) => exitOf(byId.get(e.id))))]
        : (step.after || []).length
          ? [...new Set(entryDeps)] // all its prerequisites were conditional and are off
          : entryDeps;

      let out;
      if (!condHolds(step.when, flags)) {
        out = deps;
      } else if (step.type === 'routine') {
        out = walk(step.routineId, `${prefix}${step.id}/`, deps, [...stack, routineId]);
      } else {
        const id = `${prefix}${step.id}`;
        nodes.push({ id, title: step.title || '(untitled)', deps, source: routine.name });
        out = [id];
      }
      visiting.delete(step.id);
      memo.set(step.id, out);
      return out;
    };

    routine.steps.forEach(exitOf);

    const mine = nodes.slice(start);
    if (!mine.length) return entryDeps;
    const consumed = new Set(mine.flatMap((n) => n.deps));
    return mine.filter((n) => !consumed.has(n.id)).map((n) => n.id);
  }

  walk(rootId, '', [], []);
  return nodes;
}

const resolved = (run, id) => run.done[id] === 'done' || run.done[id] === 'skipped';
const key = (runId, nodeId) => `${runId}\u0000${nodeId}`;

/** Steps nothing inside the run waits on before them. */
export const roots = (run) => run.nodes.filter((n) => !n.deps.length);
/** Steps nothing inside the run waits on after them. */
export const sinks = (run) => {
  const used = new Set(run.nodes.flatMap((n) => n.deps));
  return run.nodes.filter((n) => !used.has(n.id));
};

/**
 * Cross-run waiting created by attachments.
 * run.attach = { rel: 'after'|'before', run: otherRunId, id: otherNodeId }
 *  - after:  the run's first steps wait on that step.
 *  - before: that step waits on the run's last steps.
 * Returns Map(key(runId, nodeId) -> [{ run, id }]) of extra prerequisites.
 */
export function crossLinks(runs) {
  const map = new Map();
  const add = (toRun, toId, from) => {
    const k = key(toRun, toId);
    if (!map.has(k)) map.set(k, []);
    map.get(k).push(from);
  };
  for (const r of Object.values(runs)) {
    const a = r.attach;
    if (!a || !runs[a.run]) continue;
    if (a.rel === 'after') roots(r).forEach((n) => add(r.id, n.id, { run: a.run, id: a.id }));
    else sinks(r).forEach((n) => add(a.run, a.id, { run: r.id, id: n.id }));
  }
  return map;
}

// ctx = { runs, links } — optional; without it only a run's own steps count.
const extDeps = (ctx, run, id) => ctx?.links?.get(key(run.id, id)) || [];
const extResolved = (ctx, x) => {
  const r = ctx.runs[x.run];
  return !r || !r.nodes.some((n) => n.id === x.id) || resolved(r, x.id);
};
const isReady = (run, n, ctx) =>
  n.deps.every((d) => resolved(run, d)) && extDeps(ctx, run, n.id).every((x) => extResolved(ctx, x));

/** Steps you can do right now. */
export function ready(run, ctx) {
  return run.nodes.filter((n) => !resolved(run, n.id) && isReady(run, n, ctx));
}

/** Steps still blocked: blockedBy = own unresolved steps, blockedExt = [{run,id}] in other runs. */
export function waiting(run, ctx) {
  return run.nodes
    .filter((n) => !resolved(run, n.id) && !isReady(run, n, ctx))
    .map((n) => ({
      ...n,
      blockedBy: n.deps.filter((d) => !resolved(run, d)),
      blockedExt: extDeps(ctx, run, n.id).filter((x) => !extResolved(ctx, x)),
    }));
}

export const isFinished = (run) => run.nodes.every((n) => resolved(run, n.id));

/** Would the current attachments make steps across runs wait on each other in a loop? */
export function hasCycle(runs) {
  const links = crossLinks(runs);
  const state = new Map(); // 1 = visiting, 2 = done
  const visit = (runId, nodeId) => {
    const k = key(runId, nodeId);
    if (state.get(k) === 2) return false;
    if (state.get(k) === 1) return true;
    state.set(k, 1);
    const node = runs[runId]?.nodes.find((n) => n.id === nodeId);
    const deps = [...(node?.deps || []).map((d) => ({ run: runId, id: d })), ...(links.get(k) || [])];
    const cyc = deps.some((d) => runs[d.run] && visit(d.run, d.id));
    state.set(k, 2);
    return cyc;
  };
  return Object.values(runs).some((r) => r.nodes.some((n) => visit(r.id, n.id)));
}

/** Would making `stepId` wait on `depId` create a cycle inside one routine? */
export function wouldCycle(routine, stepId, depId) {
  if (stepId === depId) return true;
  const byId = new Map(routine.steps.map((s) => [s.id, s]));
  const seen = new Set();
  const reaches = (from) => {
    if (from === stepId) return true;
    if (seen.has(from)) return false;
    seen.add(from);
    return (byId.get(from)?.after || []).some((e) => reaches(e.id));
  };
  return reaches(depId);
}

/** Does routine `rid` (transitively) include routine `target`? */
export function includes(routines, rid, target, seen = new Set()) {
  if (rid === target) return true;
  if (seen.has(rid) || !routines[rid]) return false;
  seen.add(rid);
  return routines[rid].steps.some((s) => s.type === 'routine' && includes(routines, s.routineId, target, seen));
}
