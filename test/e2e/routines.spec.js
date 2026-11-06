import { test, expect } from '@playwright/test';
import { db, go, runs, toast, start } from './helpers.js';

test('routine page shows steps with conditions and nested sub-routines; Start works', async ({ page }) => {
  await page.goto('./#/routines');
  await expect(page.locator('#app a.btn', { hasText: 'Edit' })).toHaveCount(0); // Edit lives on the routine page
  await page.locator('a.plain', { hasText: 'Morning' }).click();
  await expect(page.locator('ol.steps').first()).toContainText('Gym session');
  await expect(page.locator('ol.steps').first()).toContainText('only if gym');
  await expect(page.locator('ol.steps ol.steps')).toContainText('Start the coffee');
  await page.locator('.wide-btn').click();
  await page.locator('dialog button.primary').click();
  await expect(page).toHaveURL(/#\/$/);
  expect(await runs(page)).toHaveLength(1);
});

test('an empty routine does not start; the toast offers Edit', async ({ page }) => {
  await page.goto('./#/routines');
  await page.locator('[data-act=newRoutine]').click();
  await expect(page).toHaveURL(/#\/edit\//);
  await go(page, '#/');
  await page.locator('.chips.start button', { hasText: 'New routine' }).click();
  expect(await runs(page)).toHaveLength(0);
  await expect(toast(page)).toContainText('has no steps yet');
  await toast(page).locator('button', { hasText: 'Edit' }).click();
  await expect(page).toHaveURL(/#\/edit\//);
});

test('an empty run left by an older version is removed on load', async ({ page }) => {
  await page.goto('./');
  await start(page, 'breakfast'); // so there is saved data to edit
  await expect(page.locator('.runcard')).toHaveCount(1);
  await page.evaluate(() => {
    const d = JSON.parse(localStorage.getItem('routines.v1'));
    d.runs.ghost = { id: 'ghost', name: 'Ghost', nodes: [], done: {}, flags: {}, startedAt: 1 };
    localStorage.setItem('routines.v1', JSON.stringify(d));
  });
  await page.reload();
  await expect(page.locator('#app')).not.toContainText('Ghost');
});

test('abandoning from the run page uses Undo, not a confirm popup', async ({ page }) => {
  let popups = 0;
  page.on('dialog', (d) => { popups++; d.accept(); });
  await page.goto('./');
  await start(page, 'breakfast');
  await page.locator('.runcard').click();
  await page.locator('[data-act=abandon]').click();
  await expect(page).toHaveURL(/#\/$/);
  expect(await runs(page)).toHaveLength(0);
  await expect(toast(page)).toContainText('Abandoned “Breakfast”');
  await toast(page).locator('button').click();
  await expect.poll(async () => (await runs(page)).length).toBe(1);
  expect(popups).toBe(0);
});

test('the editor adds a step that the next run picks up', async ({ page }) => {
  await page.goto('./#/edit/breakfast');
  await page.locator('[data-act=addStep]').click();
  await page.locator('[data-f=title]').last().fill('Wipe the table');
  await page.locator('[data-f=title]').last().press('Tab');
  expect((await db(page)).routines.breakfast.steps.at(-1)).toMatchObject({ title: 'Wipe the table', after: [{ id: 'clean' }] });
});
