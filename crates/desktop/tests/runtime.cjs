const { spawn, spawnSync } = require('node:child_process');
const { createInterface } = require('node:readline');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');

function runtime(onEvent = () => {}, options = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'herdstead-desktop-'));
  for (const dir of ['config', 'state', 'run', 'project/src', 'zsh']) fs.mkdirSync(path.join(root, dir), { recursive: true });
  fs.writeFileSync(path.join(root, 'config/config.toml'), `shell = "${options.shell || '/bin/sh'}"\nrestore = true\nscrollback = 1000\n`);
  fs.writeFileSync(path.join(root, 'project/README.md'), '# Desktop workspace\n\nShared **Markdown** and `code`.\n');
  fs.writeFileSync(path.join(root, 'project/src/main.rs'), 'fn main() { println!("hello"); }\n');
  // Runner-wide zshrc files can prompt before the fixture's zshrc is read.
  fs.writeFileSync(path.join(root, 'zsh/.zshenv'), 'unsetopt GLOBAL_RCS\n');
  fs.writeFileSync(path.join(root, 'zsh/.zshrc'), options.zshrc || `PROMPT='HS> '\n`);
  const binary = process.env.HERDSTEAD_BIN || path.resolve(__dirname, '../../../target/debug/herdstead');
  assert.ok(fs.existsSync(binary), 'Build the Herdstead runtime with cargo build first');
  const cwd = path.join(root, 'project');
  const env = { ...process.env, HERDSTEAD_CONFIG_DIR: path.join(root, 'config'), HERDSTEAD_STATE_DIR: path.join(root, 'state'), HERDSTEAD_RUNTIME_DIR: path.join(root, 'run'), HERDSTEAD_CWD: cwd, HERDSTEAD_SESSION: 'desktop-test', HERDSTEAD_BIN: binary, ZDOTDIR: path.join(root, 'zsh'), HERDSTEAD_SHELL_HELPERS: options.helpers ? '1' : '' };
  if (options.home) env.HOME = fs.realpathSync(cwd);
  const child = spawn(binary, ['-s', 'desktop-test', 'desktop-bridge'], { cwd, env });
  const pending = new Map(); let id = 0, latest, ready;
  createInterface({ input: child.stdout }).on('line', line => {
    const event = JSON.parse(line);
    if (event.type === 'reply') {
      const item = pending.get(event.id); if (!item) return;
      pending.delete(event.id); clearTimeout(item.timer);
      if (event.error) item.reject(new Error(event.error)); else item.resolve(event.value);
    } else {
      if (event.type === 'ready') ready = event;
      if (event.type === 'snapshot') latest = event;
      onEvent(event);
    }
  });
  let errors = ''; child.stderr.on('data', bytes => { errors += bytes; });
  function request(message) {
    return new Promise((resolve, reject) => {
      const requestId = ++id;
      const timer = setTimeout(() => { pending.delete(requestId); reject(new Error(errors || 'Bridge request timed out')); }, 10000);
      pending.set(requestId, { resolve, reject, timer });
      child.stdin.write(`${JSON.stringify({ ...message, id: requestId })}\n`);
    });
  }
  async function wait(predicate, label = 'runtime condition') {
    const deadline = Date.now() + 12000;
    while (Date.now() < deadline) {
      if (latest && predicate(latest)) return latest;
      await new Promise(resolve => setTimeout(resolve, 30));
    }
    throw new Error(`Timed out waiting for ${label}: ${errors}\n${JSON.stringify(latest?.snapshot.panes.map(p => p.grid?.lines)).slice(-3000)}`);
  }
  async function close() {
    child.stdin.end();
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), new Promise(resolve => setTimeout(() => { child.kill(); resolve(); }, 1500))]);
    spawnSync(binary, ['kill', 'desktop-test', '--forget'], { env, cwd });
    fs.rmSync(root, { recursive: true, force: true });
  }
  return { root, env, cwd, child, request, wait, close, latest: () => latest, events: () => [ready, latest].filter(Boolean) };
}
module.exports = { runtime };
