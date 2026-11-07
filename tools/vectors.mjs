// Fills in `expected` in every shared test vector (core/tests/vectors/*.json)
// from what src/graph.js returns today. Run after adding or changing a case:
//   npm run vectors
// and read the diff: a changed expectation is a change in behaviour.
import { writeFileSync } from 'node:fs';
import { files, call, dir } from '../test/vectors.js';

// JSON with short arrays and objects kept on one line, so cases stay readable.
// `lead` is how many characters precede the value on its line.
const WIDTH = 100;
function format(v, indent = '', lead = 0) {
  const flat = JSON.stringify(v);
  if (v === null || typeof v !== 'object' || indent.length + lead + flat.length <= WIDTH) return flat;
  const inner = indent + '  ';
  const items = Array.isArray(v)
    ? v.map((x) => inner + format(x, inner))
    : Object.entries(v).map(([k, x]) => `${inner}${JSON.stringify(k)}: ${format(x, inner, JSON.stringify(k).length + 2)}`);
  if (!items.length) return flat;
  const [open, close] = Array.isArray(v) ? ['[', ']'] : ['{', '}'];
  return `${open}\n${items.join(',\n')}\n${indent}${close}`;
}

for (const { file, data } of files()) {
  for (const c of data.cases) c.expected = call(c);
  writeFileSync(new URL(file, dir), format(data) + '\n');
}
