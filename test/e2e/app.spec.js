// The app as an installable, offline-capable PWA, and the toast's layout.
import { test, expect } from '@playwright/test';
import { swipe } from './helpers.js';

test('manifest and its icons load', async ({ page }) => {
  await page.goto('./');
  const statuses = await page.evaluate(async () => {
    const r = await fetch(document.querySelector('link[rel=manifest]').href);
    const m = await r.json();
    return Promise.all(m.icons.map((i) => fetch(new URL(i.src, r.url)).then((x) => x.status)));
  });
  expect(statuses).toEqual([200, 200, 200]);
});

test('works offline once loaded', async ({ page, context }) => {
  await page.goto('./');
  await page.evaluate(() => navigator.serviceWorker.ready);
  await page.reload(); // now controlled by the service worker
  await context.setOffline(true);
  await page.goto('./');
  await expect(page.locator('h1')).toHaveText('Today');
});

test('the undo toast fits short text on one line and keeps its layout while fading', async ({ page }) => {
  await page.goto('./#/backlog');
  await page.locator('[data-form=quickadd] input').fill('Untitled');
  await page.keyboard.press('Enter');
  await swipe(page, page.locator('[data-kind=item]'));
  const t = page.locator('#toast');
  await expect(t).toContainText('Deleted “Untitled”');
  const lineHeight = await t.locator('.msg').evaluate((e) => parseFloat(getComputedStyle(e).lineHeight));
  const before = await t.boundingBox();
  expect(before.height).toBeLessThan(lineHeight * 2 + 24); // one line plus padding
  await t.evaluate((e) => e.classList.remove('show'));
  expect((await t.boundingBox()).height).toBe(before.height);
});
