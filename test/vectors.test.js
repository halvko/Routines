import { test } from 'node:test';
import assert from 'node:assert/strict';
import { files, call } from './vectors.js';

for (const { file, data } of files()) {
  for (const c of data.cases) {
    test(`${file}: ${c.name}`, () => assert.deepEqual(call(c), c.expected));
  }
}
