//! The test page: passkeys, data, push. Also the sign-in step of the OAuth
//! flow (`/?next=/authorize?...`).

pub const INDEX: &str = r#"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Workers prototype</title>
<style>
  :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
  body { max-width: 32rem; margin: 1rem auto; padding: 0 16px; }
  input, button { font: inherit; padding: .4rem .6rem; margin: .2rem 0; }
  pre { white-space: pre-wrap; word-break: break-all; font-size: .85rem; }
</style>
<h1>Workers prototype</h1>
<p>Throwaway. Proves Rust on Cloudflare Workers for decision 0005.</p>
<label>User <input id="user" value="owner" autocomplete="username webauthn"></label><br>
<button id="register">Register passkey</button>
<button id="login">Sign in with passkey</button>
<p id="status">Signed out.</p>
<label>Routine <input id="name" value="Morning"></label>
<button id="add">Add</button>
<button id="hash">Hash one password</button>
<h2>Routines</h2><pre id="routines"></pre>
<h2>Pushed</h2><pre id="pushed"></pre>
<script>
const $ = (id) => document.getElementById(id);
const status = (s) => { $("status").textContent = s; };
async function post(path, body) {
  const r = await fetch(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  const v = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(v.error || r.statusText);
  return v;
}
async function signedIn() {
  status("Signed in as " + $("user").value + ".");
  const next = new URLSearchParams(location.search).get("next");
  if (next && next.startsWith("/authorize?")) { location.href = next; return; }
  const r = await fetch("/api/routines");
  if (r.ok) $("routines").textContent = JSON.stringify((await r.json()).routines, null, 1);
  connect();
}
let ws;
function connect() {
  if (ws) return;
  ws = new WebSocket((location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/api/ws");
  ws.onopen = () => { document.body.dataset.ws = "open"; };
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    m.receivedAt = Date.now();
    $("pushed").textContent += JSON.stringify(m) + "\n";
    if (m.routines) $("routines").textContent = JSON.stringify(m.routines, null, 1);
  };
  ws.onclose = () => { ws = null; document.body.dataset.ws = "closed"; };
}
$("register").onclick = async () => {
  try {
    const user = $("user").value;
    const ccr = await post("/api/passkey/register/start", { user });
    const cred = await navigator.credentials.create({ publicKey: PublicKeyCredential.parseCreationOptionsFromJSON(ccr.publicKey) });
    await post("/api/passkey/register/finish", { user, credential: cred.toJSON() });
    await signedIn();
  } catch (e) { status("Register failed: " + e.message); }
};
$("login").onclick = async () => {
  try {
    const user = $("user").value;
    const rcr = await post("/api/passkey/login/start", { user });
    const cred = await navigator.credentials.get({ publicKey: PublicKeyCredential.parseRequestOptionsFromJSON(rcr.publicKey) });
    await post("/api/passkey/login/finish", { user, credential: cred.toJSON() });
    await signedIn();
  } catch (e) { status("Sign-in failed: " + e.message); }
};
$("add").onclick = async () => {
  try { await post("/api/routines", { name: $("name").value }); }
  catch (e) { status("Add failed: " + e.message); }
};
$("hash").onclick = async () => {
  const t = performance.now();
  const r = await (await fetch("/hash")).json();
  status("Hashed in " + Math.round(performance.now() - t) + " ms round trip: " + r.phc);
};
</script>
</html>
"#;
