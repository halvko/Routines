import { test, expect } from '@playwright/test';
import { go, start, step, addToBacklog, runNamed, toast, runCard, hold } from './helpers.js';

test.beforeEach(async ({ page }) => { await page.goto('./'); });

async function place(page, rel, stepTitle) {
  const dlg = page.locator('dialog[open]');
  await dlg.locator(`input[value=${rel}]`).check({ force: true });
  await dlg.locator('select').selectOption({ label: stepTitle });
  await dlg.locator('button.primary').click();
}

test('a routine placed after a step waits for it', async ({ page }) => {
  await start(page, 'morning');
  await start(page, 'travel');
  await runCard(page, 'Travel').click();
  await page.locator('[data-act=place]').click();
  await place(page, 'after', 'Get dressed');
  await expect(page.locator('.placement')).toContainText('Placed after Get dressed (Morning)');
  await page.locator('[data-act=back]').click();
  await expect(page.locator('h2:text("Do now") ~ .card.step', { hasText: 'Clothes' })).toHaveCount(0);
  await page.locator('details.coming summary').click();
  await expect(step(page, 'Clothes')).toContainText('after Get dressed (Morning)');
});

test('a to-do placed before a step holds that step back until it is done', async ({ page }) => {
  await start(page, 'morning');
  await addToBacklog(page, 'Call the dentist');
  await go(page, '#/backlog');
  await page.locator('[data-act=pull]').click();
  await place(page, 'before', 'Brush teeth');
  await expect(runCard(page, 'Call the dentist')).toContainText('before Brush teeth');
  await go(page, '#/');
  for (const s of ['Get out of bed', 'Drink a glass of water', 'Shower']) await step(page, s).locator('.check').click();
  const ready = (t) => page.locator('h2:text("Do now") ~ .card.step').filter({ has: page.getByText(t, { exact: true }) });
  await expect(ready('Call the dentist')).toHaveCount(1);
  await expect(ready('Brush teeth')).toHaveCount(0);
  await step(page, 'Call the dentist').locator('.check').click();
  await expect(ready('Brush teeth')).toHaveCount(1);
});

test('placements that would loop are refused', async ({ page }) => {
  await start(page, 'morning');
  await start(page, 'travel');
  await runCard(page, 'Travel').click();
  await page.locator('[data-act=place]').click();
  await place(page, 'after', 'Get dressed');
  await page.locator('[data-act=back]').click();
  await runCard(page, 'Morning').click();
  await page.locator('[data-act=place]').click();
  await place(page, 'after', 'Clothes');
  await expect(toast(page)).toContainText('loop');
  await expect.poll(async () => (await runNamed(page, 'Morning')).attach).toBeUndefined();
});

test('removing a run frees what was placed relative to it', async ({ page }) => {
  await start(page, 'morning');
  await start(page, 'travel');
  await runCard(page, 'Travel').click();
  await page.locator('[data-act=place]').click();
  await place(page, 'after', 'Get dressed');
  await page.locator('[data-act=back]').click();
  await runCard(page, 'Morning').click();
  await page.locator('[data-act=abandon]').click();
  await expect(page.locator('h2:text("Do now") ~ .card.step').filter({ has: page.getByText('Clothes', { exact: true }) })).toHaveCount(1);
  await expect.poll(async () => (await runNamed(page, 'Travel')).attach).toBeUndefined();
});

test('Start and place… from a chip starts and places in one go', async ({ page }) => {
  await start(page, 'morning');
  await expect(runCard(page, 'Morning')).toHaveCount(1);
  await hold(page, page.locator('.chips.start [data-id=breakfast]'));
  await page.locator('dialog .sheet-btn', { hasText: 'Start and place' }).click();
  await place(page, 'after', 'Get dressed');
  await expect.poll(async () => (await runNamed(page, 'Breakfast'))?.attach?.rel).toBe('after');
});
