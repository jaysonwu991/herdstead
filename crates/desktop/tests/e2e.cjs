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
  const scrollActions = [];
  let updateResult = null;
  let updateError = null;
  let installError = null;
  let installs = 0;
  let checks = 0;
  let fullscreenCalls = 0;
  const zoomCalls = [];
  page.on('pageerror', error => failures.push(error.message));
  const run = runtime(event => { page.evaluate(event => window.__desktopEvent?.(event), event).catch(() => {}); }, { shell: '/bin/zsh', integration: true, home: true });
  try {
    await run.wait(() => true);
    await run.request({ type: 'action', action: { type: 'rename_workspace', name: 'project' } });
    await page.exposeFunction('__invoke', (name, args) => {
      if (name === 'check_update') {
        checks++;
        if (updateError) throw new Error(updateError);
        return updateResult;
      }
      if (name === 'install_update') {
        installs++;
        if (installError) throw new Error(installError);
        return null;
      }
      if (name === 'bridge_state') return run.events();
      if (name === 'set_zoom') { zoomCalls.push(args.scale); return null; }
      if (name === 'toggle_fullscreen') { fullscreenCalls++; return null; }
      if (name === 'bridge_request') {
        if (args.message.action?.type === 'scroll_pane') scrollActions.push(args.message.action);
        return run.request(args.message);
      }
      if (name === 'open_external') { openedLinks.push(args.target); return null; }
      if (name === 'choose_folder') return run.cwd;
      return null;
    });
    await page.addInitScript(() => {
      window.__TAURI__ = {
        core: { invoke: (name, args) => window.__invoke(name, args) },
        event: { listen: async (name, callback) => {
          if (name === 'herdstead:event') window.__desktopEvent = event => callback({ payload: event });
          if (name === 'herdstead:command') window.__nativeCommand = command => callback({ payload: command });
          return () => {};
        } },
        window: { getCurrentWindow: () => ({ startDragging() {} }) },
      };
    });
    await page.goto(`file://${path.resolve(__dirname, '../ui/index.html')}`);
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
    await page.waitForFunction(() => document.querySelector('#workspaces .name').textContent === '~');
    assert.equal(await page.locator('#workspaces .name').first().textContent(), '~');
    assert.equal(await page.locator('#workspaces .initial').first().textContent(), '~');
    await page.waitForFunction(() => document.querySelector('#file-breadcrumb').textContent === '~');
    for (const selector of ['#find-button', '#settings-button', '.file-filter']) {
      const size = await page.locator(`${selector} .ui-icon`).first().boundingBox();
      assert.equal(size.width, 16, `${selector} icon width`);
      assert.equal(size.height, 16, `${selector} icon height`);
    }
    const chevron = await page.locator('.file-row.directory .file-icon .ui-icon').first().boundingBox();
    const folder = await page.locator('.file-row.directory .file-icon .ui-icon').last().boundingBox();
    assert.deepEqual([chevron.width, chevron.height], [13, 13], 'Directory chevron stays compact');
    assert.deepEqual([folder.width, folder.height], [18, 18], 'Folder icon is large enough to read');
    const fileIcon = await page.locator('.file-row:not(.directory) .file-icon .ui-icon').first().boundingBox();
    assert.deepEqual([fileIcon.width, fileIcon.height], [18, 18], 'File icon matches the folder icon size');
    assert.notEqual(
      await page.locator('.file-row:not(.directory) .file-icon .ui-icon path').first().getAttribute('d'),
      await page.locator('.file-row.directory .file-icon .ui-icon').last().locator('path').getAttribute('d'),
      'Files and folders use different icons',
    );
    for (const selector of ['.brand', '#topbar']) {
      assert.equal(await page.locator(selector).getAttribute('data-tauri-drag-region'), 'deep', `${selector} is a draggable title-bar region`);
    }
    await run.request({ type: 'action', action: { type: 'rename_workspace', name: 'My home' } });
    await page.waitForFunction(() => document.querySelector('#workspaces .name').textContent === 'My home');
    await run.request({ type: 'action', action: { type: 'rename_workspace', name: 'project' } });
    await page.waitForFunction(() => document.querySelector('#workspaces .name').textContent === '~');
    assert.equal(await page.locator('#workspace-header, #terminal-tools, #pane-list').count(), 0);
    assert.ok((await page.locator('.pane').boundingBox()).y < 60, 'The terminal starts directly below the tab strip');
    await page.evaluate(() => { window.__nativeCommand('settings'); window.__nativeCommand('settings'); });
    await page.locator('#settings[open]').waitFor();
    assert.equal(await page.locator('#settings h2').textContent(), 'Settings');
    await page.locator('#close-settings').click();
    await page.evaluate(() => window.__nativeCommand('check-for-updates'));
    await page.locator('#settings[open] #update-status').filter({ hasText: 'You are up to date.' }).waitFor();
    assert.equal(checks, 1, 'Native update command checks for an update');
    assert.equal(installs, 0, 'Native update command never installs automatically');
    await page.locator('#close-settings').click();
    fs.mkdirSync(path.resolve(__dirname, '../test-results'), { recursive: true });
    await page.screenshot({ path: path.resolve(__dirname, '../test-results/workspace.png') });
    const agentPath = path.join(run.cwd, 'opencode');
    fs.writeFileSync(agentPath, '#!/bin/sh\nsleep "$@"\n', { mode: 0o755 });
    await run.request({ type: 'action', action: { type: 'send_text', text: './opencode 60', submit: true } });
    await page.waitForFunction(() => document.querySelector('#agents strong')?.textContent === 'opencode');
    assert.match(await page.locator('#agents small').textContent(), /^~ · /);
    await run.request({ type: 'action', action: { type: 'key', input: { key: { char: 'c' }, mods: { ctrl: true } } } });
    await page.waitForFunction(() => document.querySelector('#agent-count').textContent === '0' && document.querySelector('.pane-header .title').textContent === 'shell');
    assert.equal(await page.locator('#composer').count(), 0);
    await run.wait(event => { const p = event.snapshot.panes.find(p => p.id === event.snapshot.focused); const g = p?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'editable zsh prompt');
    await page.waitForFunction(() => document.activeElement?.id === 'terminal-input' && document.body.classList.contains('terminal-focused'));
    // The cursor is a cell of its own row: it sits on the empty cell after the
    // prompt instead of covering the output above it.
    await page.waitForFunction(() => {
      const cell = document.querySelector('.pane.focused .terminal-row .cursor');
      return cell && cell.textContent === ' ' && cell.parentElement.textContent.trim() === 'HS>';
    });
    assert.equal(await page.locator('.cursor').count(), 1, 'One block cursor, inside the only terminal');
    const caret = await page.evaluate(() => {
      const cell = document.querySelector('.pane.focused .cursor'), row = cell.parentElement;
      const box = cell.getBoundingClientRect(), line = row.getBoundingClientRect();
      const prompt = row.firstElementChild.getBoundingClientRect();
      return {
        gap: box.left - prompt.right, width: box.width, height: box.height,
        top: box.top - line.top, lineHeight: line.height,
        cellWidth: parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--cell-width')),
        animation: getComputedStyle(cell).animationName,
      };
    });
    assert.ok(Math.abs(caret.gap - caret.cellWidth) < 1, `The block sits one cell past the prompt, not over its text (${caret.gap}px)`);
    assert.ok(Math.abs(caret.width - caret.cellWidth) < 1, 'The block is one cell wide');
    assert.ok(Math.abs(caret.height - caret.lineHeight) < 1 && Math.abs(caret.top) < 1, 'The block fills its row cell');
    assert.equal(caret.animation, 'terminal-cursor-blink', 'The focused block blinks');
    await page.locator('#find-button').click();
    assert.equal(await page.locator('#find-input').evaluate(node => document.activeElement === node), true, 'Find retains keyboard focus');
    await page.locator('#find-close').click();
    await page.waitForFunction(() => document.activeElement?.id === 'terminal-input');
    // A logo takes the two cells free before a link — the row's gutter plus the
    // leading space, as `git` prints paths — so it never sits on a character.
    await page.keyboard.type("printf ' https://github.com\\n src/main.rs\\n'"); await page.keyboard.press('Enter');
    await page.waitForFunction(() => document.querySelectorAll('.pane.focused .link-logo').length === 2);
    const logos = await page.evaluate(() => [...document.querySelectorAll('.pane.focused .link-logo')].map(logo => {
      const box = logo.getBoundingClientRect(), link = logo.parentElement.getBoundingClientRect();
      const scale = Math.min(box.width, box.height) / 24, ink = logo.querySelector('path').getBBox();
      return {
        kind: logo.dataset.kind,
        cells: box.width / parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--cell-width')),
        gap: link.left - (box.left + (box.width - 24 * scale) / 2 + (ink.x + ink.width) * scale),
        text: logo.closest('.terminal-row').textContent,
      };
    }));
    assert.deepEqual(logos.map(logo => logo.kind), ['github', 'file'], 'A row-start link carries the logo for its kind');
    assert.deepEqual(logos.map(logo => logo.text), [' https://github.com', ' src/main.rs'], 'A logo adds no characters to the row');
    for (const logo of logos) {
      assert.ok(Math.abs(logo.cells - 2) < 0.1, `A logo fills the two free cells before it (${logo.cells})`);
      assert.ok(logo.gap > 1, `A logo keeps a gap before the text it introduces (${logo.gap.toFixed(1)}px)`);
    }
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
    assert.equal(await page.locator('.pane.focused .link-logo').count(), 2, 'A link with one free cell keeps its colour and underline, not a squeezed logo');
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
    await page.locator('.pane').first().hover();
    await page.locator('.pane-split-right').first().click(); await page.waitForFunction(() => document.querySelectorAll('.pane').length === 2);
    await page.waitForFunction(() => document.activeElement?.id === 'terminal-input');
    await page.waitForFunction(() => document.querySelectorAll('.pane .cursor').length === 2);
    const carets = await page.evaluate(() => [...document.querySelectorAll('.pane')].map(pane => {
      const style = getComputedStyle(pane.querySelector('.cursor'));
      return { focused: pane.classList.contains('focused'), animation: style.animationName, outline: style.boxShadow !== 'none' };
    }));
    assert.deepEqual(carets.find(caret => caret.focused), { focused: true, animation: 'terminal-cursor-blink', outline: false }, 'The focused terminal blinks its block');
    assert.deepEqual(carets.find(caret => !caret.focused), { focused: false, animation: 'none', outline: true }, 'A pane without keyboard focus draws a hollow block');
    const stability = await page.evaluate(async event => {
      const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
      await frame(); await frame();
      const handles = [...document.querySelectorAll('.resize-handle')];
      const titles = [...document.querySelectorAll('.pane-header .title')].map(node => node.firstChild);
      const row = document.querySelector('.terminal-row');
      for (let i = 0; i < 20; i++) { window.__desktopEvent(event); await frame(); }
      await frame();
      return {
        handles: handles.length > 0 && handles.every(node => node.isConnected),
        titles: titles.every(node => node.isConnected),
        row: row.isConnected,
        signaturesInDOM: document.querySelectorAll('[data-signature]').length,
      };
    }, run.latest());
    assert.deepEqual(stability, { handles: true, titles: true, row: true, signaturesInDOM: 0 }, 'Repeated terminal frames retain resize handles, headers and unchanged rows without copying row JSON into the DOM');
    // One click on a pane is enough: keys follow immediately, not after a second one.
    const clickedPane = page.locator('.pane').first();
    await clickedPane.locator('.rows').click();
    await page.keyboard.type('printf "ONE-CLICK\\n"'); await page.keyboard.press('Enter');
    await clickedPane.locator('.terminal-row').filter({ hasText: 'ONE-CLICK' }).first().waitFor();
    await page.evaluate(() => {
      const pane = document.querySelector('.pane');
      for (let i = 0; i < 24; i++) pane.dispatchEvent(new WheelEvent('wheel', { deltaY: -1, bubbles: true, cancelable: true }));
    });
    await page.waitForFunction(() => !scrollPending);
    await page.evaluate(() => actionQueue);
    assert.equal(scrollActions.length, 1, 'A wheel burst sends one daemon action per animation frame');
    assert.equal(scrollActions[0].delta, 72, 'Wheel batching retains the complete scroll distance');
    await run.request({ type: 'action', action: { type: 'scroll_to_bottom', pane: scrollActions[0].pane } });
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
    // Regression: renderChrome() used to tear down and rebuild every workspace/
    // tab/agent button whenever the snapshot changed, which happens
    // continuously while a terminal is actively working. A node replaced
    // between mousedown and mouseup never receives its click, so the button
    // needed a second try. Tag the live nodes, hold a press while a burst of
    // chrome-affecting updates lands (as frequent agent/tab status changes
    // would), then release: one click must still land. The new tab is active,
    // so the race targets the first tab.
    await page.evaluate(() => {
      document.querySelector('#tabs .tab').dataset.regressionTag = 'kept';
      document.querySelector('#workspaces .workspace').dataset.regressionTag = 'kept';
    });
    const firstTabBox = await page.locator('#tabs .tab').first().boundingBox();
    const renameBurst = (async () => {
      for (let i = 0; i < 30; i++) {
        await run.request({ type: 'action', action: { type: 'rename_workspace', name: i % 2 === 0 ? 'busy' : 'project' } });
      }
    })();
    await page.mouse.move(firstTabBox.x + firstTabBox.width / 2, firstTabBox.y + firstTabBox.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(150); // several chrome rebuilds land while the button is held down
    await page.mouse.up();
    await renameBurst;
    await page.waitForFunction(() => document.querySelectorAll('#tabs .tab')[0]?.classList.contains('active'), null, { timeout: 2000 })
      .catch(() => { throw new Error('Switching tabs needed a second click while the terminal was busy'); });
    assert.equal(await page.evaluate(() => document.querySelectorAll('#tabs .tab')[1].classList.contains('active')), false, 'The second tab is deactivated after one click on the first');
    assert.equal(await page.evaluate(() => document.querySelector('#tabs .tab').dataset.regressionTag), 'kept', 'Tab buttons are reused, not rebuilt, across frequent chrome updates');
    assert.equal(await page.evaluate(() => document.querySelector('#workspaces .workspace').dataset.regressionTag), 'kept', 'Workspace buttons are reused, not rebuilt, across frequent chrome updates');
    await page.locator('#tabs .tab').nth(1).click(); // restore the new tab as active for the steps below
    await page.waitForFunction(() => document.querySelectorAll('.tab')[1]?.classList.contains('active'));
    await run.wait(event => { const p = event.snapshot.panes.find(p => p.id === event.snapshot.focused); const g = p?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'editable zsh prompt');
    await page.waitForFunction(() => document.activeElement?.id === 'terminal-input');
    await page.keyboard.type('cat @src/ma'); await page.keyboard.press('Tab');
    await page.locator('.terminal-row').filter({ hasText: '@src/main.rs' }).first().waitFor();
    await page.keyboard.press('Enter');
    await page.locator('.terminal-row').filter({ hasText: 'fn main()' }).first().waitFor();
    await run.wait(event => { const g = event.snapshot.panes.find(p => p.id === event.snapshot.focused)?.grid; return g?.cursor && g.lines[g.cursor.row]?.runs.map(r => r.text).join('').trim() === 'HS>'; }, 'prompt after highlighted cat');
    await page.keyboard.type('printf "# Output heading\\n\\n**bold output**\\n"'); await page.keyboard.press('Enter');
    await page.locator('.terminal-row').filter({ hasText: 'bold output' }).first().waitFor();
    await page.locator('#settings-button').click();
    await page.locator('#rich-output-button').click();
    await page.locator('#close-settings').click();
    await page.locator('.terminal-row').filter({ hasText: /^\*\*bold output\*\*$/ }).first().waitFor();
    await page.locator('#settings-button').click();
    await page.locator('#rich-output-button').click();
    await page.locator('#close-settings').click();
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
    await page.waitForFunction(() => document.activeElement?.id === 'terminal-input');
    await page.locator('#palette-button').click(); await page.locator('#command-input').fill('Split terminal down');
    await page.keyboard.press('Enter');
    await page.evaluate(() => window.__nativeCommand('zoom-in'));
    await page.waitForFunction(() => localStorage.getItem('zoom') === '1.1');
    assert.equal(zoomCalls.at(-1), 1.1, 'The native View menu zooms the window');
    await page.keyboard.press(`${process.platform === 'darwin' ? 'Meta' : 'Control'}+-`);
    await page.waitForFunction(() => localStorage.getItem('zoom') === '1');
    await page.locator('#palette-button').click(); await page.locator('#command-input').fill('Actual size');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => !document.querySelector('#palette').open);
    await page.locator('#palette-button').click(); await page.locator('#command-input').fill('Toggle full screen');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => !document.querySelector('#palette').open);
    assert.equal(fullscreenCalls, 1, 'The full-screen command reaches the native window');
    // One press on a space must switch space, with no second try: the press is
    // what acts, so a release the webview or a drag region drops cannot lose it.
    await run.request({ type: 'action', action: { type: 'new_workspace', dir: path.join(run.root, 'project') } });
    await page.waitForFunction(() => document.querySelectorAll('#workspaces .workspace').length === 2);
    const firstSpace = await page.locator('#workspaces .workspace').first().boundingBox();
    await page.mouse.move(firstSpace.x + firstSpace.width / 2, firstSpace.y + firstSpace.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(120);
    await page.mouse.up();
    await page.waitForFunction(() => document.querySelectorAll('#workspaces .workspace')[0]?.classList.contains('active'), null, { timeout: 2000 })
      .catch(() => { throw new Error('Switching spaces needed a second click'); });
    await run.wait(event => event.snapshot.active_workspace === 0, 'space switched on one press');
    await run.request({ type: 'action', action: { type: 'select_workspace', index: 1 } });
    await page.waitForFunction(() => document.querySelectorAll('#workspaces .workspace')[1]?.classList.contains('active'));
    await page.setViewportSize({ width: 1100, height: 740 });
    await page.waitForTimeout(300);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.setViewportSize({ width: 1500, height: 960 });
    await page.waitForTimeout(300);
    fs.mkdirSync(path.resolve(__dirname, '../test-results'), { recursive: true });
    await page.screenshot({ path: path.resolve(__dirname, '../test-results/desktop.png') });
    assert.deepEqual(failures, []);
    console.log('Desktop E2E passed: real PTY input, splits, tabs, one-press spaces and focus, shell input Markdown and link logos, shell completion, files, inline Markdown, @path, themes, palette, zoom, full screen and resize.');
  } finally { await browser.close(); await run.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
