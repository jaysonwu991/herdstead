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
  const openedLinks = [];
  let updateResult = null;
  let updateError = null;
  let installError = null;
  let installs = 0;
  page.on('pageerror', error => failures.push(error.message));
  const run = runtime(event => { page.evaluate(event => window.__desktopEvent?.(event), event).catch(() => {}); }, { shell: '/bin/zsh', integration: true });
  try {
    await run.wait(() => true);
    await page.exposeFunction('__invoke', (name, args) => {
      if (name === 'check_update') {
        if (updateError) throw new Error(updateError);
        return updateResult;
      }
      if (name === 'install_update') {
        installs++;
        if (installError) throw new Error(installError);
        return null;
      }
      if (name === 'bridge_state') return run.events();
      if (name === 'bridge_request') return run.request(args.message);
      if (name === 'open_external') { openedLinks.push(args.target); return null; }
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
    assert.equal(await page.evaluate(() => [...document.fonts].some(face => face.family === 'Herdstead Mono' && face.status === 'loaded')), true, 'Bundled font loads before the PTY is sized');
    const fontProof = await page.evaluate(() => {
      const canvas = document.createElement('canvas'); canvas.width = 48; canvas.height = 48;
      const context = canvas.getContext('2d'); context.font = '24px "Herdstead Mono"';
      const pixels = character => { context.clearRect(0, 0, 48, 48); context.fillText(character, 0, 32); return Array.from(context.getImageData(0, 0, 48, 48).data).join(','); };
      const missing = pixels('\u0378');
      return ['\ue0b0', '\ue0b6', '\uf179', '\uf418', '\ue7a8', '\uf017', '\udb80\udc35'].every(character => pixels(character) !== missing);
    });
    assert.equal(fontProof, true, 'Powerline and Nerd Font symbols render as glyphs rather than missing-character boxes');
    await page.locator('.pane').waitFor();
    await page.locator('.terminal-row').filter({ hasText: 'HS>' }).first().waitFor();
    assert.equal(await page.locator('#composer').count(), 0);
    await run.wait(event => { const p = event.snapshot.panes.find(p => p.id === event.snapshot.focused); const g = p?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'editable zsh prompt');
    await page.locator('#terminal-input').focus();
    const literal = '**bold** *italic* `code` [GitHub](https://github.com) https://example.com';
    const draft = `printf '%s' '${literal}' > input-proof.txt`;
    await page.keyboard.type(draft);
    await page.waitForFunction(() => [...document.querySelectorAll('.pane.focused .terminal-row span')].some(span => span.textContent === 'bold' && span.style.fontWeight === '700'));
    await page.waitForFunction(() => [...document.querySelectorAll('.pane.focused .terminal-row span')].some(span => span.textContent === 'italic' && span.style.fontStyle === 'italic'));
    const githubLogo = page.locator('.pane.focused .link-logo[data-kind="github"]').first();
    await githubLogo.waitFor();
    assert.equal(fs.existsSync(path.join(run.cwd, 'input-proof.txt')), false);
    await page.waitForFunction(draft => document.querySelector('.pane.focused .rows')?.textContent.includes(draft), draft);
    const originalInput = await page.locator('.pane.focused .rows').textContent();
    assert.ok(originalInput.includes('**bold**'));
    await githubLogo.click();
    assert.deepEqual(openedLinks, ['https://github.com']);
    assert.equal(fs.existsSync(path.join(run.cwd, 'input-proof.txt')), false, 'Opening a link must not submit the shell input');
    assert.equal(await page.locator('.pane.focused .rows').textContent(), originalInput, 'Link logos do not add source characters');
    await page.locator('#terminal-input').focus(); await page.keyboard.press('Enter');
    await run.wait(() => fs.existsSync(path.join(run.cwd, 'input-proof.txt')), 'unchanged Markdown command execution');
    assert.equal(fs.readFileSync(path.join(run.cwd, 'input-proof.txt'), 'utf8'), literal);
    await run.wait(event => { const g = event.snapshot.panes.find(p => p.id === event.snapshot.focused)?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'editable prompt after Markdown input');
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
    await page.locator('#check-update').click();
    await page.locator('#update-status').filter({ hasText: 'You are up to date.' }).waitFor();
    assert.equal(await page.locator('#install-update').isVisible(), false);
    updateResult = { version: '1.2.3' };
    await page.locator('#check-update').click();
    await page.locator('#update-status').filter({ hasText: 'Version 1.2.3 is available.' }).waitFor();
    assert.equal(installs, 0, 'Checking must not install an update');
    installError = 'Invalid signature';
    await page.locator('#install-update').click();
    await page.locator('#update-status').filter({ hasText: 'Update failed: Invalid signature' }).waitFor();
    assert.equal(await page.locator('#check-update').isEnabled(), true);
    updateError = 'Network unavailable';
    await page.locator('#check-update').click();
    await page.locator('#update-status').filter({ hasText: 'Cannot check for updates: Network unavailable' }).waitFor();
    assert.equal(await page.locator('#install-update').isVisible(), false);
    updateError = null; installError = null;
    await page.locator('#check-update').click();
    await page.locator('#install-update').waitFor();
    await page.locator('#install-update').click();
    await page.waitForFunction(() => document.querySelector('#install-update').disabled);
    assert.equal(installs, 2);
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
    console.log('Desktop E2E passed: real PTY input, splits, tabs, shell input Markdown and link logos, shell completion, files, inline Markdown, @path, themes, palette and resize.');
  } finally { await browser.close(); await run.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
