const { test } = require('node:test');
const assert = require('node:assert/strict');
const { runtime } = require('./runtime.cjs');
const plain = event => Object.values(event.screens).flat().map(row => row.map(run => run.text).join('')).join('\n');
test('desktop bridge drives a real PTY, split, viewer and persistent session', { timeout: 30000 }, async () => {
  const run = runtime();
  try {
    await run.wait(event => event.snapshot.panes.length === 1, 'initial shell');
    const request = run.request;
    await request({ type: 'action', action: { type: 'send_text', text: 'printf "DESKTOP-PTY-OK\\n"', submit: true } });
    await run.wait(event => plain(event).includes('DESKTOP-PTY-OK'), 'shell output');
    const listing = await request({ type: 'directory', path: run.cwd });
    assert.equal(listing.entries[0].name, 'src');
    assert.ok(listing.entries.some(entry => entry.name === 'README.md'));
    await request({ type: 'action', action: { type: 'split', dir: 'vertical' } });
    await run.wait(event => event.snapshot.panes.length === 2, 'split panes');
    await request({ type: 'action', action: { type: 'open_viewer', path: `${run.cwd}/README.md` } });
    const event = await run.wait(event => event.snapshot.panes.some(pane => pane.kind === 'viewer'), 'Markdown viewer');
    const pane = event.snapshot.panes.find(pane => pane.kind === 'viewer');
    const rows = await request({ type: 'viewer', pane, width: 60 });
    assert.ok(rows.flat().some(run => run.text.includes('Desktop workspace')));
    const theme = await request({ type: 'theme', name: 'nord' });
    assert.equal(theme.name, 'nord'); assert.match(theme.palette.bg, /^#[0-9a-f]{6}$/);
    const preview = await request({ type: 'preview', text: '# Hello\n\n**world**', width: 40 });
    assert.ok(preview.flat().some(run => run.text.includes('Hello')));
    await assert.rejects(request({ type: 'directory', path: `${run.cwd}/missing` }), /opening/);
    await assert.rejects(request({ type: 'action', action: { type: 'invalid-action' } }), /invalid desktop message/);
    // A failed request must not break subsequent terminal input.
    await request({ type: 'action', action: { type: 'new_tab' } });
    await run.wait(event => event.snapshot.workspaces[0].tabs.length === 2, 'new tab after failed request');
    run.child.stdin.end();
    await new Promise(resolve => run.child.once('exit', resolve));
    const { spawnSync } = require('node:child_process');
    const inspect = spawnSync(run.env.HERDSTEAD_BIN, ['ls'], { env: run.env, cwd: run.cwd, encoding: 'utf8' });
    assert.equal(inspect.status, 0); assert.match(inspect.stdout, /desktop-test/);
  } finally { await run.close(); }
});
