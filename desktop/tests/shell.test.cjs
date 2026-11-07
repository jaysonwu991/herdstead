const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { runtime } = require('./runtime.cjs');
const shell = ['/bin/zsh', '/usr/bin/zsh'].find(file => fs.existsSync(file));
const plain = event => event.snapshot.panes.map(pane => pane.grid?.lines.map(line => line.runs.map(run => run.text).join('')).join('\n') || '').join('\n');
test('real zsh completes and expands @paths, honours zshrc and keeps pipelines raw', { skip: !shell, timeout: 30000 }, async () => {
  const run = runtime(() => {}, { shell, integration: true, zshrc: `PROMPT='USER-PROMPT> '\nexport HERDSTEAD_USER_PROMPT=kept\n` });
  try {
    await run.wait(event => plain(event).includes('USER-PROMPT>'), 'user prompt');
    const ready = () => run.wait(event => { const grid = event.snapshot.panes[0].grid; return grid.cursor && grid.lines[grid.cursor.row]?.runs.map(run => run.text).join('').trim() === 'USER-PROMPT>'; }, 'editable user prompt');
    await ready();
    const action = action => run.request({ type: 'action', action });
    await action({ type: 'send_text', text: 'printf "PATH=%s\\n" @src/ma', submit: false });
    await action({ type: 'key', input: { key: 'tab' } });
    await run.wait(event => plain(event).includes('@src/main.rs'), 'native path completion');
    await action({ type: 'key', input: { key: 'enter' } });
    await run.wait(event => plain(event).includes(`PATH=${fs.realpathSync(run.cwd)}/src/main.rs`), 'literal path expansion');
    await ready();
    await action({ type: 'send_text', text: 'cat src/main.rs | wc -c', submit: true });
    const size = fs.statSync(path.join(run.cwd, 'src/main.rs')).size;
    await run.wait(event => plain(event).split('\n').some(line => line.trim() === String(size)), 'uncoloured pipe output');
    await ready();
    await action({ type: 'send_text', text: 'cat README.md', submit: true });
    await run.wait(event => plain(event).includes('Desktop workspace') && event.snapshot.panes[0].grid.lines.some(line => line.runs.some(run => run.style?.fg)), 'rich cat output');
    await ready();
    fs.writeFileSync(path.join(run.cwd, 'a b.rs'), 'const VALUE: u32 = 42;\n');
    await action({ type: 'send_text', text: `printf 'AUTO=%s\\n' @a`, submit: false });
    await action({ type: 'key', input: { key: 'tab' } });
    await run.wait(event => plain(event).includes('@a\\ b.rs'), 'completion escapes a filename with spaces');
    await action({ type: 'key', input: { key: 'enter' } });
    await run.wait(event => plain(event).includes(`AUTO=${fs.realpathSync(run.cwd)}/a b.rs`), 'completed space reference expands literally');
    await ready();
    await action({ type: 'send_text', text: `printf 'SPACE=%s\\n' @"a b.rs"`, submit: false });
    await action({ type: 'key', input: { key: 'enter' } });
    await run.wait(event => plain(event).includes(`SPACE=${fs.realpathSync(run.cwd)}/a b.rs`), 'quoted path with spaces');
    await ready();
    await action({ type: 'send_text', text: 'echo SHOULD-NOT-RUN @missing.rs', submit: false });
    await action({ type: 'key', input: { key: 'enter' } });
    await run.wait(event => plain(event).includes('@path does not exist'), 'missing reference validation');
    assert.equal(fs.existsSync(path.join(run.cwd, 'PWNED')), false);
  } finally { await run.close(); }
});

test('shell integration preserves user aliases and normal completion widgets', { skip: !shell, timeout: 25000 }, async () => {
  const run = runtime(() => {}, { shell, integration: true, zshrc: `
autoload -Uz compinit
compinit -D -i
PROMPT='COMPAT> '
alias cat='print USER-CAT'
alias md='print USER-MD'
_user_tab() { LBUFFER+='USER-TAB'; }
zle -N _user_tab
bindkey '^I' _user_tab
` });
  try {
    const action = action => run.request({ type: 'action', action });
    await run.wait(event => plain(event).includes('COMPAT>'), 'custom prompt with aliases');
    await action({ type: 'send_text', text: 'echo ', submit: false });
    await action({ type: 'key', input: { key: 'tab' } });
    await run.wait(event => plain(event).includes('USER-TAB'), 'original completion widget');
    await action({ type: 'key', input: { key: 'enter' } });
    await action({ type: 'send_text', text: 'cat README.md; md docs; hmd README.md', submit: true });
    await run.wait(event => plain(event).includes('USER-CAT README.md') && plain(event).includes('USER-MD docs') && plain(event).includes('Desktop workspace'), 'preserved aliases and explicit rich reader');
  } finally { await run.close(); }
});
