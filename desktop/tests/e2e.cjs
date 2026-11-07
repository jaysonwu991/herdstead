// Exercises the real desktop page and daemon. Native Tauri dialogs are checked
// in the app; this harness only substitutes the window's invoke/event transport.
const { chromium } = require('playwright');
const { runtime } = require('./runtime.cjs');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL || 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1500, height: 960 } });
  const failures = [];
  page.on('pageerror', error => failures.push(error.message));
  const run = runtime(event => { page.evaluate(event => window.__desktopEvent?.(event), event).catch(() => {}); }, { shell: '/bin/zsh', integration: true });
  try {
    await run.wait(() => true);
    await page.exposeFunction('__invoke', (name, args) => {
      if (name === 'bridge_state') return run.events();
      if (name === 'bridge_request') return run.request(args.message);
      if (name === 'choose_folder') return run.cwd;
      return null;
    });
    await page.addInitScript(() => {
      window.__TAURI__ = {
        core: { invoke: (name, args) => window.__invoke(name, args) },
        event: { listen: async (_name, callback) => { window.__desktopEvent = event => callback({ payload: event }); return () => {}; } },
        window: { getCurrentWindow: () => ({ startDragging() {} }) },
      };
    });
    await page.goto(`file://${path.resolve(__dirname, '../src/index.html')}`);
    await page.locator('#connection-status').filter({ hasText: 'Connected' }).waitFor();
    await page.locator('.pane').waitFor();
    await page.locator('.terminal-row').filter({ hasText: 'HS>' }).first().waitFor();
    assert.equal(await page.locator('#composer').count(), 0);
    await run.wait(event => { const p = event.snapshot.panes.find(p => p.id === event.snapshot.focused); const g = p?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'editable zsh prompt');
    await page.locator('#terminal-input').focus();
    await page.keyboard.type('printf "UI-TERMINAL-OK\\n"'); await page.keyboard.press('Enter');
    await page.locator('.terminal-row').filter({ hasText: 'UI-TERMINAL-OK' }).first().waitFor();
    assert.equal(await page.evaluate(() => {
      const event = new KeyboardEvent('keydown', { key: 'q', metaKey: true, bubbles: true, cancelable: true });
      document.querySelector('#terminal-input').dispatchEvent(event);
      return event.defaultPrevented;
    }), false, 'Unclaimed Cmd shortcuts must stay with the native menu');
    await page.locator('#split-right').click(); await page.waitForFunction(() => document.querySelectorAll('.pane').length === 2);
    const firstPane = await page.locator('.pane').first().getAttribute('data-id');
    await page.locator('.pane').first().locator('.pane-zoom').click();
    await page.waitForFunction(id => document.querySelectorAll('.pane').length === 1 && document.querySelector('.pane.focused')?.dataset.id === id, firstPane);
    await page.locator('.pane-zoom').click();
    await page.waitForFunction(() => document.querySelectorAll('.pane').length === 2);
    await page.locator('.pane').first().locator('.pane-split-down').click();
    await page.waitForFunction(() => document.querySelectorAll('.pane').length === 3);
    await page.locator('.pane.focused .pane-close').click();
    await page.waitForFunction(() => document.querySelectorAll('.pane').length === 2);
    await page.locator('#new-tab').click(); await page.waitForFunction(() => document.querySelectorAll('.tab').length === 2);
    await run.wait(event => { const p = event.snapshot.panes.find(p => p.id === event.snapshot.focused); const g = p?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'editable zsh prompt');
    await page.locator('#terminal-input').focus();
    await page.keyboard.type('cat @src/ma'); await page.keyboard.press('Tab');
    await page.locator('.terminal-row').filter({ hasText: '@src/main.rs' }).first().waitFor();
    await page.keyboard.press('Enter');
    await page.locator('.terminal-row').filter({ hasText: 'fn main()' }).first().waitFor();
    await run.wait(event => { const g = event.snapshot.panes.find(p => p.id === event.snapshot.focused)?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'prompt after highlighted cat');
    await page.keyboard.type('printf "# Output heading\\n\\n**bold output**\\n"'); await page.keyboard.press('Enter');
    await page.locator('.terminal-row').filter({ hasText: 'bold output' }).first().waitFor();
    await page.locator('#rich-output-button').click();
    await page.locator('.terminal-row').filter({ hasText: /^\*\*bold output\*\*$/ }).first().waitFor();
    await page.locator('#rich-output-button').click();
    await page.waitForFunction(() => [...document.querySelectorAll('.terminal-row span')].some(span => span.textContent === 'bold output' && span.style.fontWeight === '700'));
    await page.locator('#file-tree .file-row').filter({ hasText: 'README.md' }).click();
    await page.locator('.viewer-screen').filter({ hasText: 'Desktop workspace' }).waitFor();
    await page.locator('#settings-button').click(); await page.locator('#theme-select').selectOption('nord');
    await page.waitForFunction(() => localStorage.getItem('theme') === 'nord');
    await page.locator('#font-family').fill('Menlo'); await page.locator('#font-family').press('Tab');
    await page.locator('#font-larger').click(); await page.locator('#close-settings').click();
    await page.locator('#palette-button').click(); await page.locator('#command-input').fill('Split terminal down');
    await page.keyboard.press('Enter');
    await page.setViewportSize({ width: 1100, height: 740 });
    await page.waitForTimeout(300);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.setViewportSize({ width: 1500, height: 960 });
    await page.waitForTimeout(300);
    fs.mkdirSync(path.resolve(__dirname, '../test-results'), { recursive: true });
    await page.screenshot({ path: path.resolve(__dirname, '../test-results/desktop.png') });
    assert.deepEqual(failures, []);
    console.log('Desktop E2E passed: real PTY input, splits, tabs, shell completion, files, inline Markdown, @path, themes, palette and resize.');
  } finally { await browser.close(); await run.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
