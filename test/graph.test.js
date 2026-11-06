import { test } from 'node:test';
import assert from 'node:assert/strict';
import { expand, collectFlags, ready, waiting, isFinished, wouldCycle, includes } from '../src/graph.js';

const t = (id, title, after = [], extra = {}) => ({ id, type: 'task', title, after: after.map((a) => (typeof a === 'string' ? { id: a } : a)), ...extra });
const sub = (id, routineId, after = [], extra = {}) => ({ id, type: 'routine', routineId, after: after.map((a) => ({ id: a })), ...extra });
const ids = (nodes) => nodes.map((n) => n.id);

const morning = {
  id: 'm', name: 'Morning',
  steps: [
    t('wake', 'Wake'),
    t('gym', 'Gym', ['wake'], { when: 'gym' }),
    t('shower', 'Shower', ['wake', 'gym']),
    t('dress', 'Dress', ['shower']),
  ],
};

test('hidden steps are bridged out', () => {
  const nodes = expand({ m: morning }, 'm', {});
  assert.deepEqual(ids(nodes), ['wake', 'shower', 'dress']);
  assert.deepEqual(nodes.find((n) => n.id === 'shower').deps, ['wake']);
});

test('flag puts the gym before the shower', () => {
  const nodes = expand({ m: morning }, 'm', { gym: true });
  assert.deepEqual(nodes.find((n) => n.id === 'shower').deps.sort(), ['gym', 'wake']);
});

test('negated condition', () => {
  const r = { id: 'r', name: 'R', steps: [t('a', 'A', [], { when: '!rain' }), t('b', 'B', ['a'])] };
  assert.deepEqual(ids(expand({ r }, 'r', {})), ['a', 'b']);
  assert.deepEqual(ids(expand({ r }, 'r', { rain: true })), ['b']);
});

test('conditional edge', () => {
  const r = { id: 'r', name: 'R', steps: [t('a', 'A'), t('b', 'B'), t('c', 'C', [{ id: 'a' }, { id: 'b', when: 'late' }])] };
  assert.deepEqual(expand({ r }, 'r', {}).find((n) => n.id === 'c').deps, ['a']);
  assert.deepEqual(expand({ r }, 'r', { late: true }).find((n) => n.id === 'c').deps.sort(), ['a', 'b']);
});

test('sub-routines are spliced in between their neighbours', () => {
  const bf = { id: 'bf', name: 'Breakfast', steps: [t('coffee', 'Coffee'), t('food', 'Food'), t('eat', 'Eat', ['coffee', 'food'])] };
  const day = { id: 'd', name: 'Day', steps: [t('dress', 'Dress'), sub('bf', 'bf', ['dress']), t('leave', 'Leave', ['bf'])] };
  const nodes = expand({ bf, d: day }, 'd', {});
  const by = Object.fromEntries(nodes.map((n) => [n.id, n]));
  assert.deepEqual(by['bf/coffee'].deps, ['dress']);
  assert.deepEqual(by['bf/food'].deps, ['dress']);
  assert.deepEqual(by['leave'].deps, ['bf/eat']);
});

test('the same routine can be included twice', () => {
  const x = { id: 'x', name: 'X', steps: [t('a', 'A')] };
  const r = { id: 'r', name: 'R', steps: [sub('one', 'x'), sub('two', 'x', ['one'])] };
  const nodes = expand({ x, r }, 'r', {});
  assert.deepEqual(ids(nodes), ['one/a', 'two/a']);
  assert.deepEqual(nodes[1].deps, ['one/a']);
});

test('hidden or empty sub-routine is bridged', () => {
  const x = { id: 'x', name: 'X', steps: [t('a', 'A', [], { when: 'never' })] };
  const r = { id: 'r', name: 'R', steps: [t('s', 'S'), sub('x', 'x', ['s']), t('e', 'E', ['x'])] };
  assert.deepEqual(expand({ x, r }, 'r', {}).find((n) => n.id === 'e').deps, ['s']);
});

test('cycles and self-inclusion are rejected', () => {
  const c = { id: 'c', name: 'C', steps: [t('a', 'A', ['b']), t('b', 'B', ['a'])] };
  assert.throws(() => expand({ c }, 'c', {}), /Circular/);
  const s = { id: 's', name: 'S', steps: [sub('me', 's')] };
  assert.throws(() => expand({ s }, 's', {}), /includes itself/);
});

test('collectFlags walks sub-routines', () => {
  const x = { id: 'x', name: 'X', steps: [t('a', 'A', [], { when: 'Abroad' })] };
  const r = { id: 'r', name: 'R', steps: [sub('x', 'x'), t('b', 'B', [{ id: 'x', when: '!rain' }], { when: 'gym' })] };
  assert.deepEqual(collectFlags({ x, r }, 'r'), ['abroad', 'gym', 'rain']);
});

test('run progression: ready / waiting / finished', () => {
  const nodes = expand({ m: morning }, 'm', {});
  const run = { nodes, done: {} };
  assert.deepEqual(ids(ready(run)), ['wake']);
  assert.deepEqual(ids(waiting(run)), ['shower', 'dress']);
  run.done.wake = 'done';
  assert.deepEqual(ids(ready(run)), ['shower']);
  run.done.shower = 'skipped';
  run.done.dress = 'done';
  assert.ok(isFinished(run));
});

test('editor guards', () => {
  assert.ok(wouldCycle(morning, 'wake', 'dress'));
  assert.ok(!wouldCycle(morning, 'dress', 'wake'));
  const a = { id: 'a', name: 'A', steps: [sub('s', 'b')] };
  const b = { id: 'b', name: 'B', steps: [] };
  assert.ok(includes({ a, b }, 'a', 'b'));
  assert.ok(!includes({ a, b }, 'b', 'a'));
});

/* ---------- attaching runs ---------- */
import { crossLinks, hasCycle } from '../src/graph.js';

const mkRun = (id, nodes, attach) => ({ id, nodes, done: {}, flags: {}, ...(attach ? { attach } : {}) });
const n = (id, deps = []) => ({ id, title: id.toUpperCase(), deps, source: 'x' });

test('attach after: first steps wait on the other run\'s step', () => {
  const day = mkRun('day', [n('wake'), n('commute', ['wake']), n('home', ['commute'])]);
  const gym = mkRun('gym', [n('unpack'), n('wash', ['unpack']), n('bottle')], { rel: 'after', run: 'day', id: 'commute' });
  const runs = { day, gym };
  const ctx = () => ({ runs, links: crossLinks(runs) });
  assert.deepEqual(ids(ready(gym, ctx())), []);
  const w = waiting(gym, ctx());
  assert.deepEqual(w.find((x) => x.id === 'unpack').blockedExt, [{ run: 'day', id: 'commute' }]);
  assert.deepEqual(w.find((x) => x.id === 'wash').blockedExt, []); // waits only on unpack
  day.done.wake = 'done';
  day.done.commute = 'done';
  assert.deepEqual(ids(ready(gym, ctx())), ['unpack', 'bottle']);
});

test('attach before: the other run\'s step waits on this run\'s last steps', () => {
  const day = mkRun('day', [n('wake'), n('leave', ['wake'])]);
  const pack = mkRun('pack', [n('shoes'), n('towel'), n('close', ['shoes', 'towel'])], { rel: 'before', run: 'day', id: 'leave' });
  const runs = { day, pack };
  const ctx = () => ({ runs, links: crossLinks(runs) });
  day.done.wake = 'done';
  assert.deepEqual(ids(ready(day, ctx())), []);
  assert.deepEqual(waiting(day, ctx())[0].blockedExt, [{ run: 'pack', id: 'close' }]);
  pack.done.shoes = pack.done.towel = 'done';
  pack.done.close = 'skipped';
  assert.deepEqual(ids(ready(day, ctx())), ['leave']);
});

test('attachment to a deleted run is ignored', () => {
  const todo = mkRun('t', [n('call')], { rel: 'after', run: 'gone', id: 'x' });
  const runs = { t: todo };
  assert.deepEqual(ids(ready(todo, { runs, links: crossLinks(runs) })), ['call']);
});

test('cross-run loops are detected', () => {
  const a = mkRun('a', [n('a1'), n('a2', ['a1'])], { rel: 'after', run: 'b', id: 'b1' });
  const b = mkRun('b', [n('b1')]);
  assert.ok(!hasCycle({ a, b }));
  b.attach = { rel: 'after', run: 'a', id: 'a2' };
  assert.ok(hasCycle({ a, b }));
});
