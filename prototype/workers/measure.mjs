// Measures a deployed prototype: Durable Object wake latency (new object,
// warm, and after idle eviction) and Argon2id round trips. CPU time per hash
// comes from Cloudflare's analytics (see README).
// Usage: PROTO_URL=https://... node measure.mjs [idleSeconds]
const base = process.env.PROTO_URL;
if (!base) throw new Error("set PROTO_URL");
const idle = Number(process.argv[2] || 0);

async function timed(path) {
  const t = performance.now();
  const r = await fetch(base + path);
  const body = await r.json();
  return { ms: Math.round(performance.now() - t), body };
}
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

await timed("/ping/warmup"); // a warm isolate and connection for what follows
const run = Date.now();
const names = Array.from({ length: 5 }, (_, i) => `m-${run}-${i}`);
const created = [], warm = [], born = {};
for (const n of names) {
  const r = await timed(`/ping/${n}`);
  created.push(r.ms);
  born[n] = r.body.born;
  warm.push((await timed(`/ping/${n}`)).ms);
}
console.log(`new Durable Object, first request (ms): ${created.join(", ")}  median ${median(created)}`);
console.log(`same object, warm (ms):                  ${warm.join(", ")}  median ${median(warm)}`);

const hashes = [];
for (let i = 0; i < 10; i++) hashes.push((await timed("/hash")).ms);
console.log(`/hash round trips (ms): ${hashes.join(", ")}  median ${median(hashes)}`);

if (idle) {
  console.log(`waiting ${idle}s for the objects to be evicted...`);
  await new Promise((r) => setTimeout(r, idle * 1000));
  await timed("/ping/warmup");
  const woke = [];
  for (const n of names) {
    const r = await timed(`/ping/${n}`);
    woke.push(`${r.ms}${r.body.born === born[n] ? " (still warm)" : ""}`);
  }
  console.log(`after ${idle}s idle (ms): ${woke.join(", ")}`);
}
