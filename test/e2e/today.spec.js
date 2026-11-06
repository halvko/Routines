import { test, expect } from '@playwright/test';
import { start, runNamed, go, step } from './helpers.js';

test.beforeEach(async ({ page }) => { await page.goto('./'); });

test('nothing running shows an empty state', async ({ page }) => {
  await expect(page.locator('#app')).toContainText('Nothing running');
});

test('a gym morning puts the gym before the shower', async ({ page }) => {
  await start(page, 'morning', ['gym']);
  const now = page.locator('h2:text("Do now") ~ .card.step');
  for (const s of ['Get out of bed', 'Drink a glass of water']) {
    await step(page, s).locator('.check').click();
  }
  await expect(now.first()).toContainText('Gym session');
  await page.locator('details.coming summary').click();
  await expect(step(page, 'Shower')).toContainText('after Gym session');
});

test('without the gym flag the gym step is gone and the shower inherits what the gym waited on', async ({ page }) => {
  await start(page, 'morning');
  await step(page, 'Get out of bed').locator('.check').click();
  await expect(page.locator('#app')).not.toContainText('Gym session');
  await page.locator('details.coming summary').click();
  // Shower comes after the gym; with the gym hidden it waits on the gym's own prerequisite instead.
  await expect(step(page, 'Shower')).toContainText('after Drink a glass of water');
});

test('steps from several runs mix in Do now, tagged with their routine', async ({ page }) => {
  await start(page, 'morning');
  await start(page, 'travel', ['abroad']);
  await expect(page).toHaveURL(/#\/$|\/$/); // starting keeps you on Today
  await expect(step(page, 'Get out of bed').locator('.from')).toHaveText('Morning');
  await expect(step(page, 'Passport').locator('.from')).toHaveText('Travel');
  await expect(page.locator('#app')).not.toContainText('Check in online'); // flying is off
});

test('sub-routine steps are tagged with both names', async ({ page }) => {
  await start(page, 'morning');
  await start(page, 'travel');
  await page.locator('details.coming summary').click();
  await expect(step(page, 'Start the coffee').locator('.from')).toHaveText('Morning · Breakfast');
});

test('grouped layout shows a section per run and is remembered', async ({ page }) => {
  await start(page, 'morning');
  await start(page, 'travel');
  await page.locator('[data-act=mode][data-mode=grouped]').click();
  await expect(page.locator('section.group')).toHaveCount(2);
  await page.reload();
  await expect(page.locator('[data-mode=grouped]')).toHaveClass(/on/);
});

test('a run finishes by itself and reopens on undo', async ({ page }) => {
  await start(page, 'breakfast');
  for (const s of ['Start the coffee', 'Make food', 'Eat', 'Clean up']) {
    await step(page, s).locator('.check').click();
  }
  await expect(page.locator('#toast')).toContainText('Breakfast done');
  expect((await runNamed(page, 'Breakfast')).finishedAt).toBeTruthy();
  const run = await runNamed(page, 'Breakfast');
  await go(page, `#/run/${run.id}`);
  await step(page, 'Clean up').locator('.check').click();
  expect((await runNamed(page, 'Breakfast')).finishedAt).toBeFalsy();
});

test('Coming up stays open after ticking a step', async ({ page }) => {
  await start(page, 'morning');
  await page.locator('details.coming summary').click();
  await step(page, 'Get out of bed').locator('.check').click();
  await expect(page.locator('details.coming')).toHaveAttribute('open', '');
});
