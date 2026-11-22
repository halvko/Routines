// Passkey registration and sign-in against the prototype, in Chromium with
// a virtual authenticator (CDP WebAuthn domain), so no hardware is needed.
// Usage: node test/passkey.mjs [baseUrl]   (default http://localhost:8787)
// The base URL must match RP_ORIGIN in wrangler.toml.

import { createRequire } from 'node:module';

// CommonJS require, so a globally installed Playwright is found too.
const { chromium } = createRequire(import.meta.url)('playwright');

const base = (process.argv[2] || 'http://localhost:8787').replace(/\/$/, '');
const user = `pk${Date.now().toString(36)}`;

const browser = await chromium.launch();
let failed = false;
try {
  const page = await browser.newPage();
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  await page.goto(`${base}/passkey.html`);

  const reg = await page.evaluate((u) => window.register(u), user);
  console.log('ok   passkey registered', JSON.stringify(reg));
  if (reg.registered !== 1) throw new Error('expected one passkey');

  const login = await page.evaluate((u) => window.signIn(u), user);
  console.log('ok   signed in with passkey', JSON.stringify(login));
  if (!login.signedIn) throw new Error('not signed in');

  // A sign-in for another user (no passkeys there) must not succeed.
  const other = await page.evaluate((u) => window.signIn(u).then(() => 'signed in', (e) => e.message), `${user}-other`);
  console.log(`ok   other user refused: ${other}`);
  if (other === 'signed in') throw new Error('other user signed in');
} catch (e) {
  failed = true;
  console.log('FAIL', e.stack || e);
} finally {
  await browser.close();
}
process.exit(failed ? 1 : 0);
