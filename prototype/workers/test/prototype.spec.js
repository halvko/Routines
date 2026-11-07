// The prototype's checks, items 1-5 of issue #3. A CDP virtual authenticator
// stands in for the phone; the OAuth client plays Claude's part.
import { test, expect } from "@playwright/test";
import { createHash, randomBytes } from "node:crypto";

const b64url = (buf) => buf.toString("base64url");
const user = `t-${Date.now()}`;
const CALLBACK = "http://localhost:33418/callback";

async function addAuthenticator(page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
}

test.describe.configure({ mode: "serial" });

let page;
test.beforeAll(async ({ browser }) => {
  page = await (await browser.newContext()).newPage();
  await addAuthenticator(page);
});

test("1. a per-user Durable Object stores data in SQLite", async ({ request }) => {
  const first = await (await request.get(`/ping/${user}`)).json();
  const second = await (await request.get(`/ping/${user}`)).json();
  expect(second.born).toBe(first.born);
  expect(second.requests).toBe(first.requests + 1);
});

test("2. passkey registration and sign-in with webauthn-rs", async () => {
  await page.goto("/");
  await page.fill("#user", user);
  await page.click("#register");
  await expect(page.locator("#status")).toHaveText(`Signed in as ${user}.`);

  // Without a session, a second passkey can't be added to the account.
  await page.context().clearCookies();
  await page.reload();
  await page.fill("#user", user);
  await page.click("#register");
  await expect(page.locator("#status")).toContainText("already has a passkey");

  await page.click("#login");
  await expect(page.locator("#status")).toHaveText(`Signed in as ${user}.`);
});

test("3. Argon2id at m=19456 KiB, t=2, p=1", async ({ request }) => {
  const times = [];
  for (let i = 0; i < 5; i++) {
    const t = performance.now();
    const r = await (await request.get("/hash")).json();
    times.push(Math.round(performance.now() - t));
    expect(r.phc).toMatch(/^\$argon2id\$v=19\$m=19456,t=2,p=1\$/);
  }
  console.log(`hash round trips (ms): ${times.join(", ")}`);
});

test("4. the Durable Object pushes over a hibernatable WebSocket", async () => {
  // A second device: same session cookie, its own socket.
  const other = await page.context().newPage();
  await other.goto("/");
  await other.fill("#user", user);
  await other.evaluate(() => connect());
  await expect(other.locator("body")).toHaveAttribute("data-ws", "open");
  await expect(page.locator("body")).toHaveAttribute("data-ws", "open");

  const name = `Evening ${Date.now()}`;
  const sent = Date.now();
  await page.fill("#name", name);
  await page.click("#add");
  await expect(other.locator("#pushed")).toContainText(name);
  await expect(page.locator("#pushed")).toContainText(name);
  console.log(`push seen on the other device after ${Date.now() - sent} ms`);
  await other.close();
});

// Against a deployed Worker: HIBERNATION_WAIT=120 leaves the socket idle long
// enough for the Durable Object to be evicted, then pushes through it.
test("4b. push still arrives after the Durable Object hibernated", async () => {
  const wait = Number(process.env.HIBERNATION_WAIT || 0);
  test.skip(!wait, "set HIBERNATION_WAIT (seconds) to run");
  test.setTimeout((wait + 60) * 1000);
  await page.evaluate(() => { ws?.close(); });
  await expect(page.locator("body")).toHaveAttribute("data-ws", "closed");
  await page.evaluate(() => connect());
  await expect(page.locator("body")).toHaveAttribute("data-ws", "open");
  const bornBefore = await page.evaluate(() => new Promise((resolve) => {
    ws.addEventListener("message", (e) => resolve(JSON.parse(e.data).born), { once: true });
    ws.send("ping");
  }));
  await page.waitForTimeout(wait * 1000);
  const name = `After sleep ${Date.now()}`;
  const t = Date.now();
  const pushed = page.evaluate((name) => new Promise((resolve) => {
    ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (JSON.stringify(m).includes(name)) resolve(m); });
  }), name);
  const res = await page.request.post("/api/routines", { data: { name } });
  expect(res.ok()).toBe(true);
  const msg = await pushed;
  console.log(`after ${wait}s idle: evicted=${msg.born !== bornBefore}, add+push took ${Date.now() - t} ms`);
  expect(msg.born).not.toBe(bornBefore); // a new instance pushed through the old socket
});

test("5. an MCP tool behind the hand-rolled OAuth flow", async ({ request, baseURL }) => {
  // Unauthenticated: 401 pointing at the protected resource metadata.
  const denied = await request.post("/mcp", { data: { jsonrpc: "2.0", id: 1, method: "tools/list" } });
  expect(denied.status()).toBe(401);
  const metaUrl = /resource_metadata="([^"]+)"/.exec(denied.headers()["www-authenticate"])[1];
  const prm = await (await request.get(metaUrl)).json();
  expect(prm.resource).toBe(`${baseURL}/mcp`);
  const as = await (await request.get(`${prm.authorization_servers[0]}/.well-known/oauth-authorization-server`)).json();
  expect(as.code_challenge_methods_supported).toEqual(["S256"]);

  // Dynamic client registration; a non-allowlisted redirect is refused.
  const bad = await request.post(as.registration_endpoint, { data: { redirect_uris: ["https://evil.example/cb"] } });
  expect(bad.status()).toBe(400);
  const reg = await request.post(as.registration_endpoint, {
    data: { client_name: "Test client", redirect_uris: [CALLBACK] },
  });
  expect(reg.status()).toBe(201);
  const { client_id } = await reg.json();

  // Authorize in the signed-in browser, PKCE S256.
  const verifier = b64url(randomBytes(32));
  const challenge = b64url(createHash("sha256").update(verifier).digest());
  const state = b64url(randomBytes(8));
  const authorize = new URL(as.authorization_endpoint);
  for (const [k, v] of Object.entries({
    response_type: "code", client_id, redirect_uri: CALLBACK.replace("33418", "40000"),
    code_challenge: challenge, code_challenge_method: "S256", state, scope: "routines offline_access",
    resource: prm.resource,
  })) authorize.searchParams.set(k, v);
  // Nothing listens on the callback; the redirect request itself is enough.
  const allow = async () => {
    await page.goto(authorize.href);
    await expect(page.locator("h1")).toContainText("Allow Test client");
    const [req] = await Promise.all([
      page.waitForRequest((r) => r.url().startsWith("http://localhost:40000/")),
      page.click("#allow"),
    ]);
    await page.waitForURL((u) => !u.pathname.startsWith("/authorize"));
    return new URL(req.url());
  };
  let redirected = await allow();
  expect(redirected.searchParams.get("state")).toBe(state);
  expect(redirected.searchParams.get("iss")).toBe(as.issuer);
  const code = redirected.searchParams.get("code");

  const token = (form) => request.post(as.token_endpoint, { form });
  const wrong = await token({ grant_type: "authorization_code", code, client_id, redirect_uri: redirected.origin + "/callback", code_verifier: b64url(randomBytes(32)) });
  expect(wrong.status()).toBe(400); // wrong verifier, and the code is now spent
  // Run the flow again with the right verifier.
  redirected = await allow();
  const tokens = await (await token({
    grant_type: "authorization_code", code: redirected.searchParams.get("code"), client_id,
    redirect_uri: authorize.searchParams.get("redirect_uri"), code_verifier: verifier,
  })).json();
  expect(tokens.token_type).toBe("Bearer");

  // MCP, initialize-era (2025-11-25).
  const rpc = (body, headers = {}) =>
    request.post("/mcp", { data: body, headers: { authorization: `Bearer ${tokens.access_token}`, ...headers } });
  const init = await (await rpc({
    jsonrpc: "2.0", id: 1, method: "initialize",
    params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "test", version: "0" } },
  })).json();
  expect(init.result.protocolVersion).toBe("2025-11-25");
  expect(init.result.capabilities.tools).toBeTruthy();
  expect((await rpc({ jsonrpc: "2.0", method: "notifications/initialized" })).status()).toBe(202);
  const v = { "mcp-protocol-version": "2025-11-25" };
  const list = await (await rpc({ jsonrpc: "2.0", id: 2, method: "tools/list" }, v)).json();
  expect(list.result.tools.map((t) => t.name)).toEqual(["list_routines"]);
  expect(list.result.resultType).toBeUndefined();
  const call = await (await rpc({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "list_routines", arguments: {} } }, v)).json();
  expect(call.result.content[0].text).toContain("Evening");

  // MCP, 2026-07-28: no initialize, version in _meta.
  const meta = { _meta: { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } };
  const discover = await (await rpc({ jsonrpc: "2.0", id: 4, method: "server/discover", params: meta })).json();
  expect(discover.result.supportedVersions).toContain("2026-07-28");
  const list2 = await (await rpc({ jsonrpc: "2.0", id: 5, method: "tools/list", params: meta }, { "mcp-protocol-version": "2026-07-28" })).json();
  expect(list2.result.resultType).toBe("complete");

  // Refresh tokens rotate; reusing an old one revokes the family.
  const refreshed = await (await token({ grant_type: "refresh_token", refresh_token: tokens.refresh_token, client_id })).json();
  expect(refreshed.access_token).toBeTruthy();
  expect((await token({ grant_type: "refresh_token", refresh_token: tokens.refresh_token, client_id })).status()).toBe(400);
  expect((await token({ grant_type: "refresh_token", refresh_token: refreshed.refresh_token, client_id })).status()).toBe(400);
});

test("5b. /authorize without a session sends the user to sign in first", async ({ browser, request }) => {
  const { client_id } = await (await request.post("/register", { data: { redirect_uris: [CALLBACK] } })).json();
  const fresh = await (await browser.newContext()).newPage();
  const q = new URLSearchParams({
    response_type: "code", client_id, redirect_uri: CALLBACK, code_challenge_method: "S256",
    code_challenge: b64url(createHash("sha256").update("x".repeat(43)).digest()),
  });
  await fresh.goto(`/authorize?${q}`);
  expect(new URL(fresh.url()).searchParams.get("next")).toBe(`/authorize?${q}`);
  await expect(fresh.locator("#login")).toBeVisible();
  await fresh.close();
});
