// The Rust core starts new users from core/src/seed.json, a copy of seed()
// in src/store.js. They must not drift apart while both exist.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { seed } from '../src/store.js';

test('the Rust core seeds the same routines as the app', () => {
  const copy = JSON.parse(readFileSync(new URL('../core/src/seed.json', import.meta.url), 'utf8'));
  assert.deepEqual(copy, seed());
});
