const { test } = require('node:test');
const assert = require('node:assert/strict');
const { keyInput, plain } = require('../ui/terminal.js');
test('terminal keys preserve control modifiers and application cursor semantics', () => {
  assert.deepEqual(keyInput({ key: 'c', ctrlKey: true }).key, { char: 'c' });
  assert.equal(keyInput({ key: 'c', ctrlKey: true }).mods.ctrl, true);
  assert.equal(keyInput({ key: 'ArrowUp', altKey: true }).key, 'up');
  assert.equal(keyInput({ key: 'Tab', shiftKey: true }).key, 'back_tab');
  assert.deepEqual(keyInput({ key: 'F12' }).key, { f: 12 });
  assert.deepEqual(keyInput({ key: '😀' }).key, { char: '😀' });
});
test('IME and dead keys are left to the composition input path', () => {
  for (const event of [{ key: 'Process' }, { key: 'Dead' }, { key: '文', isComposing: true }, { key: 'Meta' }]) assert.equal(keyInput(event), null);
});
test('copy output retains Unicode and row boundaries', () => {
  assert.equal(plain([[{ text: '日本' }, { text: '語' }], [], [{ text: '<script>' }]]), '日本語\n\n<script>');
});

test('link logos are selected locally without favicon requests', () => {
  const { linkKind } = require('../ui/terminal.js');
  assert.equal(linkKind({ url: true, target: 'https://github.com/org/repo' }), 'github');
  assert.equal(linkKind({ url: true, target: 'https://example.com' }), 'web');
  assert.equal(linkKind({ url: true, target: 'mailto:hello@example.com' }), 'mail');
  assert.equal(linkKind({ url: false, target: 'docs/desktop.md' }), 'file');
});
