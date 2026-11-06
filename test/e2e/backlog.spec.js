import { test, expect } from '@playwright/test';
import { db, go, addToBacklog, start, step, runNamed } from './helpers.js';

test.beforeEach(async ({ page }) => { await page.goto('./#/backlog'); });

const row = (page, title) => page.locator('[data-kind=item]').filter({ has: page.getByText(title, { exact: true }) });
const quickAdd = async (page, title) => {
  await page.locator('[data-form=quickadd] input').fill(title);
  await page.keyboard.press('Enter');
};

test('capture with the + button, the inline field, and the Add button', async ({ page }) => {
  await addToBacklog(page, 'Call the dentist');
  await quickAdd(page, 'Renew passport');
  await page.locator('[data-form=quickadd] input').fill('Fix bike');
  await page.locator('[data-form=quickadd] button').click();
  expect((await db(page)).backlog.map((i) => i.title).sort()).toEqual(['Call the dentist', 'Fix bike', 'Renew passport']);
  await expect(page.locator('#backlog-count')).toHaveText('3');
});

test('starred items sort first', async ({ page }) => {
  await quickAdd(page, 'A');
  await quickAdd(page, 'B');
  await row(page, 'A').locator('[data-act=star]').click();
  await expect(page.locator('[data-kind=item]').first()).toContainText('A');
});

test('Enter in the item dialog saves rather than deletes', async ({ page }) => {
  await quickAdd(page, 'Buy gift');
  await row(page, 'Buy gift').locator('[data-act=item]').click();
  await page.locator('dialog input[name=title]').fill('Buy gift for Anna');
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await db(page)).backlog.map((i) => i.title)).toEqual(['Buy gift for Anna']);
});

test('ticking in the backlog moves the item to Done, and undo brings it back', async ({ page }) => {
  await quickAdd(page, 'Fix bike');
  await row(page, 'Fix bike').locator('.check').click();
  await expect(page.locator('details[data-key=done] summary')).toContainText('Done · 1');
  await page.locator('details[data-key=done] summary').click();
  await page.locator('[data-act=itemUndo]').click();
  await expect(row(page, 'Fix bike')).toHaveCount(1);
});

test('pulling with nothing running goes straight into Today; Back to backlog returns it', async ({ page }) => {
  await quickAdd(page, 'Renew passport');
  await row(page, 'Renew passport').locator('[data-act=pull]').click();
  await expect(page.locator('dialog[open]')).toHaveCount(0);
  await go(page, '#/');
  await page.locator('.runcard', { hasText: 'Renew passport' }).click();
  await page.locator('[data-act=unpull]').click();
  await expect(page).toHaveURL(/#\/backlog/);
  await expect(row(page, 'Renew passport')).toHaveCount(1);
});

test('finishing a pulled to-do in Today marks the backlog item done', async ({ page }) => {
  await quickAdd(page, 'Call the dentist');
  await row(page, 'Call the dentist').locator('[data-act=pull]').click();
  await go(page, '#/');
  await step(page, 'Call the dentist').locator('.check').click();
  expect((await db(page)).backlog[0].doneAt).toBeTruthy();
  expect((await runNamed(page, 'Call the dentist')).finishedAt).toBeTruthy();
});

test('pulled to-dos are tagged To-do when other things run', async ({ page }) => {
  await go(page, '#/');
  await start(page, 'breakfast');
  await addToBacklog(page, 'Water plants');
  await go(page, '#/backlog');
  await row(page, 'Water plants').locator('[data-act=pull]').click();
  await page.locator('dialog button.primary').click(); // Right away
  await go(page, '#/');
  await expect(step(page, 'Water plants').locator('.from')).toHaveText('To-do');
});
