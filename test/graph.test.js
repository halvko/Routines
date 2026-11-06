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
