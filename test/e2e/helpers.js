// Shared steps for the browser tests. Gestures are driven with the mouse,
// which goes through the same pointer events as touch.
import { expect } from '@playwright/test';

/** Everything the app has saved. */
export const db = (page) => page.evaluate(() => JSON.parse(localStorage.getItem('routines.v1') || 'null'));

export const runs = async (page) => Object.values((await db(page))?.runs || {});
export const runNamed = async (page, name) => (await runs(page)).find((r) => r.name === name);

/** Navigate inside the app (a hash change, like tapping a link). */
export async function go(page, hash) {
  await page.evaluate((h) => { location.hash = h; }, hash);
}

/** Start a seed routine from Today; `flags` are the option checkboxes to tick, if it has any. */
export async function start(page, id, flags = []) {
  await page.locator(`.chips.start [data-act=start][data-id=${id}]`).click();
  const dlg = page.locator('dialog[open]');
  if (await dlg.count()) {
    for (const f of flags) await dlg.locator(`input[name=${f}]`).check();
    await dlg.locator('button.primary').click();
  }
}

export async function addToBacklog(page, title) {
  await page.locator('.fab').click();
  await page.locator('dialog input[name=title]').fill(title);
  await page.keyboard.press('Enter');
  await expect(page.locator('dialog[open]')).toHaveCount(0);
}

/** Swipe a row left by `dist` px. */
export async function swipe(page, locator, dist = 220) {
  await locator.scrollIntoViewIfNeeded();
  const r = await locator.boundingBox();
  const y = r.y + r.height / 2;
  const x = r.x + r.width - 30;
  await page.mouse.move(x, y);
  await page.mouse.down();
  for (let i = 1; i <= 10; i++) await page.mouse.move(x - (dist * i) / 10, y + 1);
  await page.mouse.up();
  await page.waitForTimeout(250); // the slide-out animation
}

/** Press and hold a row until the action sheet opens. */
export async function hold(page, locator) {
  await locator.scrollIntoViewIfNeeded();
  const r = await locator.boundingBox();
  await page.mouse.move(r.x + r.width / 2, r.y + r.height / 2);
  await page.mouse.down();
  await page.waitForTimeout(550);
  await page.mouse.up();
}

export const sheet = (page) => page.locator('dialog.sheet[open]');
export const toast = (page) => page.locator('#toast');

/** The step row whose title is exactly `title` (not one that merely mentions it, like "after …"). */
export const step = (page, title) =>
  page.locator('[data-kind=step], .card.step').filter({ has: page.locator('.title').getByText(title, { exact: true }) });

/** The running-routine card titled exactly `name`. */
export const runCard = (page, name) =>
  page.locator('.runcard').filter({ has: page.locator('strong, span.grow').getByText(name, { exact: true }) });
