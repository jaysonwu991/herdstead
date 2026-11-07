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
  function linkKind(link) {
    if (!link.url || link.target.startsWith('file:')) return 'file';
    try {
      const url = new URL(link.target);
      if (url.protocol === 'mailto:') return 'mail';
      if (['github.com', 'gist.github.com'].includes(url.hostname)) return 'github';
    } catch {}
    return 'web';
  }
  function linkLogo(link, document) {
    const kind = linkKind(link);
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true');
    svg.classList.add('link-logo'); svg.dataset.kind = kind;
    svg.style.left = `calc(-${link.badge_offset} * var(--cell-width))`;
    const paths = {
      github: 'M9 19c-4 1-4-2-6-3m12 6v-4c0-1 .2-1.7-.5-2.4 3-.3 6-1.4 6-6.6 0-1.5-.5-2.7-1.4-3.7.1-.4.6-1.9-.2-3.3 0 0-1.2-.4-3.9 1.4a13 13 0 0 0-7 0C4.3 1.6 3.1 2 3.1 2c-.8 1.4-.3 2.9-.2 3.3C2 6.3 1.5 7.5 1.5 9c0 5.2 3 6.3 6 6.6-.5.5-.6 1.1-.5 2.4v4',
      web: 'M3 12h18M12 3c5 5 5 13 0 18M12 3c-5 5-5 13 0 18M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0',
      file: 'M14 3H5v18h14V8l-5-5v5h5M8 12h8M8 16h8',
      mail: 'M3 5h18v14H3zM3 5l9 8 9-8',
    };
    const path = document.createElementNS(svg.namespaceURI, 'path');
    path.setAttribute('d', paths[kind]); svg.append(path); return svg;
  }
  function rowElement(runs, document, onLink) {
    const row = document.createElement('div');
    row.className = 'terminal-row';
    let previousLink = null;
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
        span.setAttribute('role', 'link'); span.tabIndex = previousLink === run.link.target ? -1 : 0;
        span.setAttribute('aria-label', `Open ${run.link.target}`);
        span.title = `⌘/Ctrl-click to open ${run.link.target}`;
        if (Number.isFinite(run.link.badge_offset)) span.prepend(linkLogo(run.link, document));
        span.addEventListener('click', event => {
          if (event.metaKey || event.ctrlKey || event.target.closest('.link-logo')) { event.preventDefault(); onLink(run.link); }
        });
        span.addEventListener('keydown', event => {
          if (event.key === 'Enter') { event.preventDefault(); onLink(run.link); }
        });
      }
      previousLink = run.link?.target || null;
      row.append(span);
    }
    if (!runs.length) row.append(document.createTextNode('\u200b'));
    return row;
  }
  // Keep unchanged rows in place so output bursts do not destroy selections.
  const rowSignatures = new WeakMap();
  function updateRows(container, rows, document, onLink) {
    rows.forEach((runs, index) => {
      const signature = JSON.stringify(runs);
      const existing = container.children[index];
      if (existing && rowSignatures.get(existing) === signature) return;
      const row = rowElement(runs, document, onLink);
      rowSignatures.set(row, signature);
      if (existing) existing.replaceWith(row); else container.append(row);
    });
    while (container.children.length > rows.length) container.lastElementChild.remove();
  }
  const api = { keyInput, plain, linkKind, rowElement, updateRows };
  if (typeof module !== 'undefined') module.exports = api;
  else root.Terminal = api;
})(globalThis);
