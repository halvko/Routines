// Runs the shared test vectors (core/tests/vectors/*.json) against src/graph.js.
// The Rust core runs the same files (core/tests/vectors.rs); `npm run vectors`
// rewrites each case's `expected` from what this file computes.
import { readFileSync, readdirSync } from 'node:fs';
import * as g from '../src/graph.js';

export const dir = new URL('../core/tests/vectors/', import.meta.url);

/** Every vector file, as { file, data }. */
export const files = () => readdirSync(dir).filter((f) => f.endsWith('.json')).sort()
  .map((file) => ({ file, data: JSON.parse(readFileSync(new URL(file, dir), 'utf8')) }));

// ready/waiting see placements only when the case asks for them (`placed`).
const ctx = ({ runs, placed }) => (placed ? { runs, links: g.crossLinks(runs) } : undefined);

const calls = {
  parseCond: (i) => g.parseCond(i.cond),
  condHolds: (i) => g.condHolds(i.cond, i.flags),
  collectFlags: (i) => g.collectFlags(i.routines, i.root),
  expand: (i) => {
    try { return g.expand(i.routines, i.root, i.flags); } catch (e) { return { error: e.message }; }
  },
  roots: (i) => g.roots(i.run),
  sinks: (i) => g.sinks(i.run),
  isFinished: (i) => g.isFinished(i.run),
  ready: (i) => g.ready(i.runs[i.run], ctx(i)),
  waiting: (i) => g.waiting(i.runs[i.run], ctx(i)),
  // A Map with composite keys has no JSON form; list its entries in order.
  crossLinks: (i) => [...g.crossLinks(i.runs)].map(([k, from]) => {
    const [run, id] = k.split('\u0000');
    return { run, id, from };
  }),
  hasCycle: (i) => g.hasCycle(i.runs),
  wouldCycle: (i) => g.wouldCycle(i.routine, i.stepId, i.depId),
  includes: (i) => g.includes(i.routines, i.rid, i.target),
};

/** What graph.js returns for one case, as plain JSON. */
export function call({ fn, input }) {
  if (!calls[fn]) throw new Error(`Unknown function "${fn}"`);
  // A round trip through JSON drops `undefined` and copies, as storage would.
  return JSON.parse(JSON.stringify(calls[fn](structuredClone(input))));
}
