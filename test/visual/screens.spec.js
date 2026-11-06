// Captures every main screen at phone size, light and dark, for the visual
// reviewer. Not a test of behaviour: each case only sets a screen up and
// saves a picture to $SHOTS_DIR (default ./screenshots).
import { test } from '@playwright/test';
import { start, addToBacklog, go, hold, swipe, runCard } from '../e2e/helpers.js';

const DIR = process.env.SHOTS_DIR || 'screenshots';

for (const scheme of ['light', 'dark']) {
  test.describe(scheme, () => {
    test.use({ colorScheme: scheme });
    const shot = (page, name, opts = {}) => page.screenshot({ path: `${DIR}/${name}-${scheme}.png`, ...opts });

    test('today', async ({ page }) => {
      await page.goto('./');
      await shot(page, 'today-empty');
      await start(page, 'morning', ['gym']);
      await start(page, 'travel', ['abroad']);
      await addToBacklog(page, 'Call the dentist');
      await page.locator('.card.step .check').first().click();
      await page.locator('details.coming summary').click();
      await page.waitForTimeout(2500); // let toasts fade
      await shot(page, 'today-mixed', { fullPage: true });
      await page.locator('[data-mode=grouped]').click();
      await shot(page, 'today-grouped', { fullPage: true });
    });

    test('run and placing', async ({ page }) => {
      await page.goto('./');
      await start(page, 'morning');
      await start(page, 'travel');
      await runCard(page, 'Travel').click();
      await shot(page, 'run', { fullPage: true });
      await page.locator('[data-act=place]').click();
      await page.locator('dialog input[value=after]').check({ force: true });
      await shot(page, 'place-dialog');
    });

    test('backlog, sheet, swipe, toast', async ({ page }) => {
      await page.goto('./#/backlog');
      for (const t of ['Renew passport', 'Buy a birthday present for Anna and wrap it nicely before Saturday', 'Fix bike']) {
        await page.locator('[data-form=quickadd] input').fill(t);
        await page.keyboard.press('Enter');
      }
      await page.locator('[data-kind=item] [data-act=star]').first().click();
      await page.locator('[data-kind=item] .check').last().click();
      await page.waitForTimeout(2500);
      await shot(page, 'backlog', { fullPage: true });
      await hold(page, page.locator('[data-kind=item]').first());
      await shot(page, 'action-sheet');
      await page.keyboard.press('Escape');
      // mid-swipe: hold the pointer down while capturing
      const row = page.locator('[data-kind=item]').first();
      const r = await row.boundingBox();
      await page.mouse.move(r.x + r.width - 30, r.y + r.height / 2);
      await page.mouse.down();
      for (let i = 1; i <= 8; i++) await page.mouse.move(r.x + r.width - 30 - i * 22, r.y + r.height / 2);
      await shot(page, 'swipe');
      await page.mouse.up();
      await page.waitForTimeout(300);
      await swipe(page, page.locator('[data-kind=item]').first());
      await shot(page, 'undo-toast');
    });

    test('routines and editor', async ({ page }) => {
      await page.goto('./#/routines');
      await shot(page, 'routines');
      await go(page, '#/routine/morning');
      await shot(page, 'routine', { fullPage: true });
      await go(page, '#/edit/morning');
      await shot(page, 'editor', { fullPage: true });
    });
  });
}
