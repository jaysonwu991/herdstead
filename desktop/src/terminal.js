(function (root) {
  const keys = { Enter: 'enter', Escape: 'esc', Tab: 'tab', Backspace: 'backspace', Delete: 'delete', Insert: 'insert', ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down', Home: 'home', End: 'end', PageUp: 'page_up', PageDown: 'page_down' };
  function keyInput(event) {
    if (event.isComposing || event.key === 'Process' || event.key === 'Dead') return null;
    let key = keys[event.key];
    if (event.key === 'Tab' && event.shiftKey) key = 'back_tab';
    if (/^F([1-9]|1[0-2])$/.test(event.key)) key = { f: Number(event.key.slice(1)) };
    if (!key && [...event.key].length === 1) key = { char: event.key };
    if (!key) return null;
    return { key, mods: { shift: !!event.shiftKey, ctrl: !!event.ctrlKey, alt: !!event.altKey, sup: !!event.metaKey }, repeat: !!event.repeat };
  }
  function plain(rows) { return rows.map(row => row.map(run => run.text).join('')).join('\n'); }
  function rowElement(runs, document, onLink) {
    const row = document.createElement('div');
    row.className = 'terminal-row';
    for (const run of runs) {
      const span = document.createElement('span');
      span.textContent = run.text;
      const fg = run.fg || 'var(--fg)', bg = run.bg || 'var(--bg)';
      span.style.color = run.inverse ? bg : fg;
      if (run.bg || run.inverse) span.style.backgroundColor = run.inverse ? fg : bg;
      if (run.bold) span.style.fontWeight = '700';
      if (run.dim) span.style.opacity = '.6';
      if (run.italic) span.style.fontStyle = 'italic';
      span.style.textDecoration = [run.underline && 'underline', run.strike && 'line-through'].filter(Boolean).join(' ');
      if (run.link) {
        span.className = 'term-link';
        span.title = `⌘/Ctrl-click to open ${run.link.target}`;
        span.addEventListener('click', event => {
          if (event.metaKey || event.ctrlKey) { event.preventDefault(); onLink(run.link); }
        });
      }
      row.append(span);
    }
    if (!runs.length) row.append(document.createTextNode('\u200b'));
    return row;
  }
  // Keep unchanged rows in place so output bursts do not destroy selections.
  function updateRows(container, rows, document, onLink) {
    rows.forEach((runs, index) => {
      const signature = JSON.stringify(runs);
      const existing = container.children[index];
      if (existing?.dataset.signature === signature) return;
      const row = rowElement(runs, document, onLink);
      row.dataset.signature = signature;
      if (existing) existing.replaceWith(row); else container.append(row);
    });
    while (container.children.length > rows.length) container.lastElementChild.remove();
  }
  const api = { keyInput, plain, rowElement, updateRows };
  if (typeof module !== 'undefined') module.exports = api;
  else root.Terminal = api;
})(globalThis);
