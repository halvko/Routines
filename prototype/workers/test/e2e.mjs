// End-to-end checks against a running prototype (wrangler dev or a deploy).
// Usage: node test/e2e.mjs [baseUrl]   (default http://127.0.0.1:8787)
// Uses a fresh user per run, so it can run against the same deploy again.

import assert from 'node:assert/strict';

const base = (process.argv[2] || 'http://127.0.0.1:8787').replace(/\/$/, '');
const user = `t${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
const results = [];

async function check(name, fn) {
  const t0 = performance.now();
  try {
    const note = await fn();
    results.push({ name, ok: true, ms: Math.round(performance.now() - t0), note });
    console.log(`ok   ${name}${note ? `  (${note})` : ''}`);
  } catch (e) {
    results.push({ name, ok: false, error: String(e) });
    console.log(`FAIL ${name}\n     ${e.stack || e}`);
  }
}

const json = async (res) => {
  const text = await res.text();
  assert.ok(res.ok, `${res.status} ${text}`);
  return JSON.parse(text);
};
const push = (u, body) =>
  fetch(`${base}/u/${u}/push`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  }).then(json);
const pull = (u, after = 0) => fetch(`${base}/u/${u}/pull?after=${after}`).then(json);

let rpcId = 0;
async function mcp(method, params, { token = `dev-${user}`, notify = false } = {}) {
  const body = { jsonrpc: '2.0', method, ...(params ? { params } : {}) };
  if (!notify) body.id = ++rpcId;
  const res = await fetch(`${base}/mcp`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json, text/event-stream',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  });
  if (notify) return res;
  return json(res);
}

function openSocket(u) {
  const url = `${base.replace(/^http/, 'ws')}/u/${u}/ws`;
  const ws = new WebSocket(url);
  const messages = [];
  const waiters = [];
  ws.addEventListener('message', (e) => {
    const msg = typeof e.data === 'string' ? e.data : String(e.data);
    messages.push(msg);
    waiters.splice(0).forEach((w) => w());
  });
  const opened = new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', () => reject(new Error('socket error')), { once: true });
  });
  // Resolves with the first message matching `pred` (including ones that came already).
  const next = (pred, ms = 5000) =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`no matching message in ${ms} ms; got ${messages}`)), ms);
      const look = () => {
        const hit = messages.find(pred);
        if (hit) {
          clearTimeout(timer);
          resolve(hit);
        } else waiters.push(look);
      };
      look();
    });
  return { ws, opened, next, messages };
}

console.log(`base ${base}, user ${user}\n`);

await check('push appends in order and numbers each change', async () => {
  const r = await push(user, {
    clientId: 'phone',
    mutations: [
      { clientSeq: 1, name: 'addTodo', args: { title: 'Buy milk' } },
      { clientSeq: 2, name: 'completeStep', args: { run: 'r1', step: 's1' } },
    ],
  });
  assert.deepEqual(r.results.map((x) => [x.clientSeq, x.seq, x.duplicate]), [[1, 1, false], [2, 2, false]]);
  assert.equal(r.lastSeq, 2);
});

await check('a retried push is idempotent by (clientId, clientSeq)', async () => {
  const r = await push(user, {
    clientId: 'phone',
    mutations: [
      { clientSeq: 2, name: 'completeStep', args: { run: 'r1', step: 's1' } },
      { clientSeq: 3, name: 'addTodo', args: { title: 'Call mum' } },
    ],
  });
  assert.deepEqual(r.results.map((x) => [x.clientSeq, x.seq, x.duplicate]), [[2, 2, true], [3, 3, false]]);
  assert.equal(r.lastSeq, 3);
});

await check('pull returns everything after N, in order', async () => {
  const all = await pull(user, 0);
  assert.deepEqual(all.events.map((e) => e.seq), [1, 2, 3]);
  assert.equal(all.events[0].args.title, 'Buy milk');
  const tail = await pull(user, 2);
  assert.deepEqual(tail.events.map((e) => e.seq), [3]);
  assert.equal(tail.lastSeq, 3);
});

await check('each user has their own object and log', async () => {
  const other = await pull(`${user}-other`, 0);
  assert.deepEqual(other.events, []);
  assert.equal(other.lastSeq, 0);
});

const laptop = openSocket(user);
await check('a push pokes the user\'s other devices over a WebSocket', async () => {
  await laptop.opened;
  const t0 = performance.now();
  await push(user, { clientId: 'phone', mutations: [{ clientSeq: 4, name: 'addTodo', args: { title: 'Water plants' } }] });
  const msg = JSON.parse(await laptop.next((m) => m.includes('"lastSeq":4')));
  assert.deepEqual(msg, { type: 'poke', lastSeq: 4 });
  return `poke ${Math.round(performance.now() - t0)} ms after push started`;
});

await check('keepalive ping is answered (auto-response, no wake)', async () => {
  laptop.ws.send('ping');
  await laptop.next((m) => m === 'pong');
});

await check('MCP rejects a request without a token', async () => {
  const res = await fetch(`${base}/mcp`, { method: 'POST', body: '{}' });
  assert.equal(res.status, 401);
});

await check('MCP initialize (2025-11-25 era)', async () => {
  const r = await mcp('initialize', {
    protocolVersion: '2025-11-25',
    capabilities: {},
    clientInfo: { name: 'e2e', version: '0' },
  });
  assert.equal(r.result.protocolVersion, '2025-11-25');
  assert.ok(r.result.capabilities.tools);
  const n = await mcp('notifications/initialized', undefined, { notify: true });
  assert.equal(n.status, 202);
});

await check('MCP server/discover (2026-07-28 era)', async () => {
  const r = await mcp('server/discover', { _meta: { 'io.modelcontextprotocol/protocolVersion': '2026-07-28' } });
  assert.ok(r.result, JSON.stringify(r));
  assert.ok(r.result.supportedVersions.includes('2026-07-28'));
  return r.result.supportedVersions.join(', ');
});

await check('MCP tools/list', async () => {
  const r = await mcp('tools/list', {});
  assert.deepEqual(r.result.tools.map((t) => t.name).sort(), ['add_todo', 'read_log']);
});

await check('an MCP write goes through the same log and pokes devices', async () => {
  const r = await mcp('tools/call', { name: 'add_todo', arguments: { title: 'Book dentist' } });
  assert.equal(r.result.isError, false, JSON.stringify(r));
  assert.match(r.result.content[0].text, /#5/);
  await laptop.next((m) => m.includes('"lastSeq":5'));
  const tail = await pull(user, 4);
  assert.equal(tail.events[0].clientId, 'mcp');
  assert.equal(tail.events[0].args.title, 'Book dentist');
});

await check('MCP read_log sees changes from every device', async () => {
  const r = await mcp('tools/call', { name: 'read_log', arguments: { after: 0 } });
  const log = JSON.parse(r.result.content[0].text);
  assert.deepEqual(log.events.map((e) => e.clientId), ['phone', 'phone', 'phone', 'phone', 'mcp']);
});

await check('MCP unknown method is a JSON-RPC error', async () => {
  const r = await mcp('prompts/list', {});
  assert.equal(r.error.code, -32601);
});

laptop.ws.close();

await check('Argon2id (m=19 MiB, t=2) runs inside the runtime', async () => {
  // Warm both routes first, then compare round trips.
  await fetch(`${base}/bench/noop`);
  await fetch(`${base}/bench/argon2`);
  const time = async (path) => {
    const t0 = performance.now();
    const res = await fetch(`${base}${path}`);
    const body = await res.text();
    return { ms: performance.now() - t0, status: res.status, body };
  };
  const noop = [], hash = [];
  let last;
  for (let i = 0; i < 5; i++) {
    noop.push((await time('/bench/noop')).ms);
    const h = await time('/bench/argon2');
    assert.equal(h.status, 200, h.body);
    hash.push(h.ms);
    last = JSON.parse(h.body);
    assert.equal(last.params, 'argon2id m=19456 t=2 p=1');
    assert.match(last.hashPrefix, /^[0-9a-f]{8}$/);
  }
  const med = (a) => a.sort((x, y) => x - y)[Math.floor(a.length / 2)];
  return `median round trip ${Math.round(med(hash))} ms vs ${Math.round(med(noop))} ms no-op; in-request ${last.inRequestMs} ms`;
});

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length ? 1 : 0);
