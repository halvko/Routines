import { test, expect } from '@playwright/test';
import { db, go, start, step, swipe, hold, sheet, toast, runNamed } from './helpers.js';

const item = (page, title) => page.locator('[data-kind=item]').filter({ has: page.getByText(title, { exact: true }) });

test.describe('backlog rows', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('./#/backlog');
    for (const t of ['Call the dentist', 'Renew passport', 'Fix bike']) {
      await page.locator('[data-form=quickadd] input').fill(t);
      await page.keyboard.press('Enter');
    }
  });

  test('swipe left deletes, Undo restores', async ({ page }) => {
    await swipe(page, item(page, 'Fix bike'));
    expect((await db(page)).backlog.map((i) => i.title)).not.toContain('Fix bike');
    await toast(page).locator('button', { hasText: 'Undo' }).click();
    expect((await db(page)).backlog.map((i) => i.title)).toContain('Fix bike');
  });

  test('a short swipe snaps back and deletes nothing', async ({ page }) => {
    await swipe(page, item(page, 'Fix bike'), 40);
    expect((await db(page)).backlog).toHaveLength(3);
    await expect(item(page, 'Fix bike')).not.toHaveAttribute('style', /translate/);
  });

  test('a vertical drag neither deletes nor opens the sheet', async ({ page }) => {
    const r = await item(page, 'Renew passport').boundingBox();
    await page.mouse.move(r.x + 200, r.y + 20);
    await page.mouse.down();
    for (let i = 1; i <= 8; i++) await page.mouse.move(r.x + 200 - i, r.y + 20 + i * 12);
    await page.mouse.up();
    expect((await db(page)).backlog).toHaveLength(3);
    await expect(sheet(page)).toHaveCount(0);
  });

  test('hold opens the sheet, and its buttons work right after the hold', async ({ page }) => {
    await hold(page, item(page, 'Renew passport'));
    await expect(sheet(page).locator('.sheet-btn')).toHaveText(['Done', 'Do today…', '★ Star', 'Edit', 'Delete', 'Cancel']);
    await sheet(page).locator('.sheet-btn', { hasText: 'Star' }).click();
    await expect.poll(async () => (await db(page)).backlog.find((i) => i.title === 'Renew passport').starred).toBe(true);
  });

  test('tapping outside closes the sheet, and normal taps keep working', async ({ page }) => {
    await hold(page, item(page, 'Fix bike'));
    await page.mouse.click(195, 40);
    await expect(sheet(page)).toHaveCount(0);
    await item(page, 'Fix bike').locator('.check').click();
    expect((await db(page)).backlog.find((i) => i.title === 'Fix bike').doneAt).toBeTruthy();
  });

  test('right-click opens the same sheet', async ({ page }) => {
    await item(page, 'Call the dentist').locator('.label').click({ button: 'right' });
    await expect(sheet(page)).toHaveCount(1);
  });
});

test.describe('Today rows', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('./');
    await start(page, 'morning');
    await expect(page.locator('.runcard')).toHaveCount(1);
  });

  test('swiping a routine step drops it; the next step becomes ready; Undo un-drops', async ({ page }) => {
    await swipe(page, step(page, 'Get out of bed'));
    expect((await runNamed(page, 'Morning')).done.wake).toBe('skipped');
    await expect(page.locator('h2:text("Do now") ~ .card.step').first()).toContainText('Drink a glass of water');
    await toast(page).locator('button').click();
    expect((await runNamed(page, 'Morning')).done.wake).toBeUndefined();
  });

  test('holding a waiting step offers Done anyway', async ({ page }) => {
    await page.locator('details.coming summary').click();
    await hold(page, step(page, 'Shower'));
    await expect(sheet(page).locator('.sheet-btn')).toHaveText(['Done anyway', 'Skip', 'Open Morning', 'Place Morning…', 'Drop', 'Cancel']);
  });

  test('swiping a run card abandons it; Undo brings it back', async ({ page }) => {
    await swipe(page, page.locator('[data-kind=run]'));
    expect(await runNamed(page, 'Morning')).toBeUndefined();
    await toast(page).locator('button').click();
    expect(await runNamed(page, 'Morning')).toBeTruthy();
  });

  test('swiping a start chip does nothing', async ({ page }) => {
    await swipe(page, page.locator('.chips.start [data-id=morning]'), 150);
    await expect(page.locator('.swipe-under')).toHaveCount(0);
    expect((await db(page)).routines.morning).toBeTruthy();
  });
});

test('a routine used inside another cannot be deleted', async ({ page }) => {
  await page.goto('./#/routines');
  await swipe(page, page.locator('[data-kind=routine]', { hasText: 'Breakfast' }));
  await expect(toast(page)).toContainText('Used by Morning');
  await expect(page.locator('[data-kind=routine]', { hasText: 'Breakfast' })).toHaveCount(1);
});
