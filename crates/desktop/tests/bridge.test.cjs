const { test } = require('node:test');
const assert = require('node:assert/strict');
const { runtime } = require('./runtime.cjs');
const plain = event => Object.values(event.screens).flat().map(row => row.map(run => run.text).join('')).join('\n');
test('output bursts deliver the final screen after a slow consumer resumes', { timeout: 30000 }, async () => {
  const run = runtime();
  try {
    await run.wait(() => true, 'initial shell');
    run.child.stdout.pause();
    run.child.stdin.write(`${JSON.stringify({ id: 1000, type: 'action', action: { type: 'send_text', text: "i=0; while [ $i -lt 2000 ]; do printf 'BURST-%04d\\n' $i; i=$((i+1)); done; printf 'BURST-DONE\\n'", submit: true } })}\n`);
    await new Promise(resolve => setTimeout(resolve, 250));
    run.child.stdout.resume();
    const event = await run.wait(event => /^BURST-DONE$/m.test(plain(event)), 'final burst output');
    assert.match(plain(event), /^BURST-1999$/m);
    await run.request({ type: 'action', action: { type: 'send_text', text: "printf 'RESPONSIVE\\n'", submit: true } });
    await run.wait(event => /^RESPONSIVE$/m.test(plain(event)), 'input after output burst');
  } finally { run.child.stdout.resume(); await run.close(); }
});

test('detaching finishes an in-flight snapshot before closing stdout', { timeout: 30000 }, async () => {
  const events = [];
  const run = runtime(event => events.push(event.type));
  try {
    await run.wait(() => true, 'initial shell');
    await run.request({ type: 'action', action: { type: 'resize', size: { rows: 200, cols: 400 }, area: { x: 0, y: 0, width: 400, height: 200 } } });
    await run.wait(event => event.snapshot.panes[0].grid.rows > 100, 'large terminal snapshot');
    await run.request({ type: 'action', action: { type: 'send_text', text: "i=0; while [ $i -lt 200 ]; do printf '%0350d\\n' 0; i=$((i+1)); done", submit: true } });
    await run.wait(event => event.snapshot.panes[0].grid.lines.filter(line => line.runs.some(run => run.text.length >= 350)).length > 100, 'filled terminal snapshot');
    const closed = new Promise(resolve => run.child.once('close', resolve));
    // Backpressure keeps the event writer busy when stdin reaches EOF.
    run.child.stdout.pause();
    run.child.stdin.write(`${JSON.stringify({ id: 1000, type: 'rich_output', enabled: false })}\n`);
    await new Promise(resolve => setTimeout(resolve, 100));
    run.child.stdin.end();
    await new Promise(resolve => setTimeout(resolve, 100));
    run.child.stdout.resume();
    await closed;
    assert.equal(run.child.exitCode, 0);
    assert.equal(events.at(-1), 'disconnected');
  } finally { await run.close(); }
});

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
