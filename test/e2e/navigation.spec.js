import { test, expect } from '@playwright/test';
import { start, addToBacklog, go } from './helpers.js';

const back = (page) => page.locator('button.back');

test('opening a run hides the tabs and +; back returns to Today', async ({ page }) => {
  await page.goto('./');
  await start(page, 'morning');
  await page.locator('.runcard', { hasText: 'Morning' }).click();
  await expect(page.locator('.tabs')).toBeHidden();
  await expect(page.locator('.fab')).toBeHidden();
  await expect(back(page)).toHaveText('‹ Today');
  await back(page).click();
  await expect(page.locator('.tabs')).toBeVisible();
  await expect(page).toHaveURL(/#\/$/);
});

test('coming back restores the scroll position', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 600 });
  await page.goto('./');
  await start(page, 'morning');
  await start(page, 'travel');
  await page.locator('details.coming summary').click();
  await page.evaluate(() => window.scrollTo(0, 99999));
  const y = await page.evaluate(() => window.scrollY);
  expect(y).toBeGreaterThan(100);
  await page.locator('.runcard', { hasText: 'Travel' }).click();
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
  await back(page).click();
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(y - 5);
});

test('back names where it goes: Backlog', async ({ page }) => {
  await page.goto('./');
  await addToBacklog(page, 'Call the dentist');
  await go(page, '#/backlog');
  await page.locator('[data-act=pull]').click();
  await page.locator('.runcard', { hasText: 'Call the dentist' }).click();
  await expect(back(page)).toHaveText('‹ Backlog');
  await back(page).click();
  await expect(page).toHaveURL(/#\/backlog$/);
});

test('Routines → routine → Edit → Done → back lands on Routines, not the editor', async ({ page }) => {
  await page.goto('./#/routines');
  await page.locator('a.plain', { hasText: 'Morning' }).click();
  await expect(back(page)).toHaveText('‹ Routines');
  await page.locator('#app a.btn', { hasText: 'Edit' }).click();
  await expect(back(page)).toHaveText('‹ Morning');
  await page.locator('button', { hasText: 'Done' }).click();
  await expect(page).toHaveURL(/#\/routine\/morning$/);
  await back(page).click();
  await expect(page).toHaveURL(/#\/routines$/);
});

test('the system back button agrees with the in-app one', async ({ page }) => {
  await page.goto('./#/routines');
  await page.locator('a.plain', { hasText: 'Travel' }).click();
  await expect(page).toHaveURL(/routine\/travel/);
  await page.goBack();
  await expect(page).toHaveURL(/#\/routines$/);
  await expect(page.locator('.tabs')).toBeVisible();
});

test('reopening the app within an hour returns to the open run; after that, Today', async ({ page }) => {
  await page.goto('./');
  await start(page, 'morning');
  await page.locator('.runcard', { hasText: 'Morning' }).click();
  await expect(page).toHaveURL(/#\/run\//);
  const runUrl = page.url();
  await page.goto('./');
  await expect(page).toHaveURL(runUrl);
  await expect(back(page)).toHaveText('‹ Today');
  await page.evaluate(() => {
    const l = JSON.parse(localStorage.getItem('routines.lastRoute'));
    l.at -= 2 * 3600e3;
    localStorage.setItem('routines.lastRoute', JSON.stringify(l));
  });
  await page.goto('./');
  await expect(page.locator('h1')).toHaveText('Today');
});
