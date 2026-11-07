/* global Terminal */
const $ = selector => document.querySelector(selector);
const api = window.herdstead;
const state = {
  snapshot: null, screens: {}, connected: false, theme: '', themes: [], cwd: '',
  fontSize: Number(localStorage.getItem('fontSize')) || 14,
  files: localStorage.getItem('files') !== 'false', rich: localStorage.getItem('richOutput') !== 'false',
  fontFamily: localStorage.getItem('fontFamily') || '',
  panes: new Map(), fileRoot: '', fileEntries: [], directoryCache: new Map(),
  expanded: new Set(), paletteIndex: 0, commands: [], cell: { width: 8.4, height: 21 },
  dimensions: '', renderPending: false, directoryGeneration: 0, viewerCache: new Map(),
};
let actionQueue = Promise.resolve();
const action = value => {
  // Native invokes may run concurrently; terminal keystrokes must stay ordered.
  const result = actionQueue.then(() => request({ type: 'action', action: value }));
  actionQueue = result.catch(() => {});
  return result;
};
async function request(message) {
  try { return await api.request(message); }
  catch (error) { showError(error.message); throw error; }
}
// Event handlers report errors once, without leaking unhandled rejections.
function safely(fn) { return (...args) => { try { return Promise.resolve(fn(...args)).catch(() => {}); } catch (error) { showError(error.message); } }; }
function on(selector, name, fn) { $(selector).addEventListener(name, safely(fn)); }
function button(text, title, fn, className = '') {
  const node = document.createElement('button');
  node.textContent = text; node.title = title; node.className = className;
  node.addEventListener('click', safely(fn)); return node;
}
function el(tag, className, text) {
  const node = document.createElement(tag); if (className) node.className = className;
  if (text !== undefined) node.textContent = text; return node;
}
function paneIcon(button, kind) {
  const paths = {
    right: 'M4 5h16v14H4z M12 5v14',
    down: 'M4 5h16v14H4z M4 12h16',
    zoom: 'M14 4h6v6 M20 4l-7 7 M10 20H4v-6 M4 20l7-7',
    close: 'M6 6l12 12 M18 6L6 18',
  };
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true');
  const path = document.createElementNS(svg.namespaceURI, 'path');
  path.setAttribute('d', paths[kind]); svg.append(path); button.replaceChildren(svg);
}
function showError(message) {
  $('#banner span').textContent = message; $('#banner').hidden = false;
}
function focused() { return state.snapshot?.panes.find(p => p.id === state.snapshot.focused); }
function workspace() { return state.snapshot?.workspaces[state.snapshot.active_workspace]; }
function terminalFocus() { if (!$('#palette').open && !$('#settings').open) $('#terminal-input').focus({ preventScroll: true }); }
function colorStatus(status) { return status === 'working' ? 'var(--accent)' : status === 'error' ? 'var(--error)' : 'var(--success)'; }
function applyPalette(palette) {
  for (const [slot, value] of Object.entries(palette)) document.documentElement.style.setProperty(`--${slot}`, value);
  document.documentElement.style.colorScheme = Number.parseInt(palette.bg.slice(1, 3), 16) > 170 ? 'light' : 'dark';
}
async function setTheme(name) {
  const result = await request({ type: 'theme', name });
  applyPalette(result.palette); state.theme = result.name;
  localStorage.setItem('theme', result.name); $('#theme-select').value = result.name;
  state.viewerCache.clear(); scheduleRender();
}
function fontSize(delta = 0) {
  state.fontSize = Math.max(10, Math.min(24, state.fontSize + delta));
  localStorage.setItem('fontSize', state.fontSize);
  document.documentElement.style.setProperty('--font-size', `${state.fontSize}px`);
  document.documentElement.style.setProperty('--line-height', `${Math.ceil(state.fontSize * 1.5)}px`);
  $('#font-size').textContent = `${state.fontSize} px`;
  const family = state.fontFamily.replace(/[\\";\r\n]/g, '').slice(0, 100);
  const font = `${family ? `"${family}", ` : ''}"Herdstead Mono", "SFMono-Regular", Menlo, "DejaVu Sans Mono", monospace`;
  document.documentElement.style.setProperty('--terminal-font', font);
  const measure = el('span', '', '0000000000');
  measure.style.cssText = `position:fixed;visibility:hidden;font-size:${state.fontSize}px`;
  measure.style.fontFamily = font;
  document.body.append(measure);
  state.cell = { width: measure.getBoundingClientRect().width / 10, height: Math.ceil(state.fontSize * 1.5) };
  measure.remove();
  document.documentElement.style.setProperty('--cell-width', `${state.cell.width}px`);
  resize(); scheduleRender();
}
function resize() {
  if (!state.connected) return;
  const bounds = $('#panes').getBoundingClientRect();
  const cols = Math.max(8, Math.min(1000, Math.floor(bounds.width / state.cell.width)));
  const rows = Math.max(5, Math.min(500, Math.floor(bounds.height / state.cell.height)));
  const dimensions = `${cols}×${rows}`;
  if (dimensions === state.dimensions) return;
  state.dimensions = dimensions;
  safely(() => action({ type: 'resize', size: { rows, cols }, area: { x: 0, y: 0, width: cols, height: rows } }))();
}
let resizeTimer;
new ResizeObserver(() => { clearTimeout(resizeTimer); resizeTimer = setTimeout(resize, 70); }).observe($('#panes'));
function scheduleRender() {
  if (state.renderPending) return;
  state.renderPending = true;
  requestAnimationFrame(() => { state.renderPending = false; render(); });
}
function renderChrome() {
  const snap = state.snapshot, ws = workspace();
  if (!snap || !ws) return;
  $('#workspace-name').textContent = ws.name;
  $('#workspace-path').textContent = ws.dir;
  $('#session-name').textContent = snap.session;
  $('#terminal-status').textContent = `${snap.panes.length} ${snap.panes.length === 1 ? 'pane' : 'panes'} · ${snap.zoomed ? 'focus mode' : 'live terminal'}`;
  $('#pane-count').textContent = snap.panes.length;
  $('#dimensions').textContent = focused()?.grid ? `${focused().grid.cols} × ${focused().grid.rows} · UTF-8` : 'Rich viewer';
  const workspaces = $('#workspaces'); workspaces.replaceChildren();
  snap.workspaces.forEach((item, index) => {
    const node = button('', item.dir, () => action({ type: 'select_workspace', index }), `workspace ${index === snap.active_workspace ? 'active' : ''}`);
    node.append(el('span', 'initial', item.name.slice(0, 1).toUpperCase()), el('span', 'name', item.name), el('span', 'dot'));
    node.lastChild.style.background = colorStatus(item.status); workspaces.append(node);
  });
  const tabs = $('#tabs'); tabs.replaceChildren();
  ws.tabs.forEach((tab, index) => {
    const node = button('', `Switch to ${tab.name}`, () => action({ type: 'select_tab', index }), `tab ${index === ws.active_tab ? 'active' : ''}`);
    const close = el('span', 'close-tab', '×'); close.title = `Close ${tab.name}`;
    close.addEventListener('click', safely(async event => {
      event.stopPropagation();
      await action({ type: 'select_tab', index }); await action({ type: 'close_tab' });
    }));
    node.append(el('span', '', '❯_'), el('span', '', tab.name), close); tabs.append(node);
  });
  const paneList = $('#pane-list'); paneList.replaceChildren();
  snap.panes.forEach((pane, index) => {
    const node = button('', `Focus ${pane.title}`, () => focusPane(pane.id), pane.id === snap.focused ? 'active' : '');
    node.append(el('span', 'pane-index', String(index + 1).padStart(2, '0')), el('span', 'pane-title', pane.title), el('span', 'dot'));
    node.lastChild.style.background = colorStatus(pane.status); paneList.append(node);
  });
  const agents = snap.workspaces.flatMap((item, index) => item.tabs.filter(tab => tab.agent).map(tab => ({ ...tab, workspaceIndex: index, workspaceName: item.name, tabIndex: item.tabs.indexOf(tab) })));
  $('#agent-count').textContent = agents.length;
  $('#agents').replaceChildren();
  if (!agents.length) $('#agents').append(el('p', 'empty', 'Agents appear here when they start working in a terminal. Your herd keeps running when you close the window.'));
  agents.forEach(agent => {
    const node = button('', `Open ${agent.agent}`, async () => {
      await action({ type: 'select_workspace', index: agent.workspaceIndex });
      await action({ type: 'select_tab', index: agent.tabIndex });
    }, 'agent');
    const info = el('span', 'agent-info'); info.append(el('strong', '', agent.agent), el('small', '', `${agent.workspaceName} · ${agent.status}`));
    node.append(el('span', 'agent-icon', '✳'), info, el('span', 'dot'));
    node.lastChild.style.background = colorStatus(agent.status); $('#agents').append(node);
  });
  if (state.fileRoot !== ws.dir) {
    state.fileRoot = ws.dir; state.expanded.clear(); state.directoryCache.clear();
    safely(loadFiles)();
  }
}
let chromeSignature = '';
function render() {
  if (!state.snapshot) return;
  // Terminal frames change frequently; rebuild chrome only when its state changes.
  const signature = JSON.stringify([state.snapshot.workspaces, state.snapshot.focused, state.snapshot.panes.map(p => [p.id, p.title, p.status]), state.snapshot.zoomed]);
  if (signature !== chromeSignature) { chromeSignature = signature; renderChrome(); }
  $('#dimensions').textContent = focused()?.grid ? `${focused().grid.cols} × ${focused().grid.rows} · UTF-8` : 'Rich viewer';
  const snap = state.snapshot, live = new Set(snap.panes.map(p => p.id));
  for (const [id, view] of state.panes) if (!live.has(id)) { view.node.remove(); state.panes.delete(id); state.viewerCache.delete(id); }
  $('#panes .connecting')?.remove();
  for (const pane of snap.panes) {
    let view = state.panes.get(pane.id);
    if (!view) {
      const node = el('section', 'pane'); node.dataset.id = pane.id;
      const header = el('header', 'pane-header'), title = el('span', 'title'), cwd = el('span', 'cwd'), offset = el('span', 'offset');
      const controls = el('div', 'pane-controls');
      controls.setAttribute('role', 'toolbar');
      controls.setAttribute('aria-label', 'Pane controls');
      const paneAction = async value => {
        await action({ type: 'focus_pane', pane: pane.id });
        await action(value); terminalFocus();
      };
      const splitRight = button('◧', 'Split pane right', () => paneAction({ type: 'split', dir: 'vertical' }), 'pane-split-right');
      const splitDown = button('⬒', 'Split pane down', () => paneAction({ type: 'split', dir: 'horizontal' }), 'pane-split-down');
      const zoom = button('⛶', 'Zoom pane', () => paneAction({ type: 'toggle_zoom' }), 'pane-zoom');
      const close = button('×', 'Close pane', () => action({ type: 'kill_pane', pane: pane.id }), 'pane-close');
      paneIcon(splitRight, 'right'); paneIcon(splitDown, 'down'); paneIcon(zoom, 'zoom'); paneIcon(close, 'close');
      for (const control of [splitRight, splitDown, zoom, close]) control.setAttribute('aria-label', control.title);
      controls.append(splitRight, splitDown, zoom, close);
      header.append(el('span', '', pane.kind === 'viewer' ? '▤' : '❯_'), title, cwd, offset, controls);
      const screen = el('div', `screen ${pane.kind === 'viewer' ? 'viewer-screen' : ''}`);
      screen.setAttribute('role', 'log'); screen.setAttribute('aria-label', `Output for ${pane.title}`);
      const lines = el('div', 'rows'), cursor = el('div', 'cursor');
      screen.append(lines, cursor); node.append(header, screen);
      node.addEventListener('mousedown', safely(async event => {
        if (event.target.closest('button')) return;
        if (state.snapshot.focused !== pane.id) await action({ type: 'focus_pane', pane: pane.id });
      }));
      node.addEventListener('mouseup', () => { if (!window.getSelection()?.toString()) terminalFocus(); });
      node.addEventListener('wheel', safely(event => {
        if (pane.kind === 'viewer') return;
        event.preventDefault();
        return action({ type: 'scroll_pane', pane: pane.id, delta: event.deltaY < 0 ? 3 : -3, page: false });
      }), { passive: false });
      $('#panes').append(node); view = { node, title, cwd, offset, screen, lines, cursor, zoom }; state.panes.set(pane.id, view);
    }
    const cell = state.cell, rect = pane.rect;
    Object.assign(view.node.style, { left: `${(rect.x - snap.area.x) * cell.width}px`, top: `${(rect.y - snap.area.y) * cell.height}px`, width: `${rect.width * cell.width}px`, height: `${rect.height * cell.height}px` });
    view.node.classList.toggle('focused', pane.id === snap.focused);
    view.zoom.setAttribute('aria-pressed', snap.zoomed && pane.id === snap.focused);
    view.zoom.title = snap.zoomed ? 'Restore pane layout' : 'Zoom pane';
    view.zoom.setAttribute('aria-label', view.zoom.title);
    view.title.textContent = pane.title;
    view.cwd.textContent = pane.cwd; view.cwd.title = pane.cwd;
    view.offset.textContent = pane.grid?.offset ? `↑ ${pane.grid.offset} · click for live` : pane.exit_code !== null ? `exit ${pane.exit_code}` : '';
    view.offset.onclick = () => safely(() => action({ type: 'scroll_to_bottom', pane: pane.id }))();
    const onLink = link => safely(() => openLink(link, pane.cwd))();
    if (pane.grid) {
      Terminal.updateRows(view.lines, state.screens[pane.id] || [], document, onLink);
      const cursor = pane.grid.cursor;
      view.cursor.hidden = !cursor || pane.grid.offset > 0;
      if (cursor) Object.assign(view.cursor.style, { top: `${cursor.row * cell.height}px`, left: `${(cursor.col + 1) * cell.width}px` });
    } else safely(() => renderViewer(pane, view))();
  }
  renderDividers(); find();
}
async function renderViewer(pane, view) {
  const width = Math.max(4, pane.rect.width - 2), key = `${pane.viewer?.path}:${width}:${state.theme}`;
  let cached = state.viewerCache.get(pane.id);
  if (cached?.pending || (cached?.key === key && Date.now() - cached.time < 1000)) return;
  cached = { key, pending: true, time: Date.now() }; state.viewerCache.set(pane.id, cached);
  view.cursor.hidden = true;
  try {
    const rows = await request({ type: 'viewer', pane, width });
    if (state.viewerCache.get(pane.id) !== cached) return;
    Terminal.updateRows(view.lines, rows, document, link => safely(() => openLink(link, pane.cwd))());
  } catch (error) { view.lines.replaceChildren(el('div', 'notice error', error.message)); }
  finally { cached.pending = false; }
}
setInterval(() => { if (state.snapshot?.panes.some(p => p.kind === 'viewer')) scheduleRender(); }, 1000);
async function openLink(link, base) {
  if (link.url && link.target.startsWith('file:')) {
    const url = new URL(link.target);
    if (url.hostname && url.hostname !== 'localhost') throw new Error('Only local file links can be opened');
    return action({ type: 'open_viewer', path: decodeURIComponent(url.pathname) });
  }
  if (link.url) return api.external(link.target);
  const path = link.target.startsWith('/') ? link.target : `${base}/${link.target}`;
  return action({ type: 'open_viewer', path });
}
async function focusPane(id) { await action({ type: 'focus_pane', pane: id }); terminalFocus(); }
function renderDividers() {
  // Keep handles stable during pointer capture.
  if (document.body.dataset.resizing) return;
  document.querySelectorAll('.resize-handle').forEach(node => node.remove());
  const panes = state.snapshot.panes, cell = state.cell;
  for (let i = 0; i < panes.length; i++) for (let j = i + 1; j < panes.length; j++) {
    const a = panes[i], b = panes[j], ar = a.rect, br = b.rect;
    let axis, start, length, coordinate, target;
    const left = ar.x < br.x ? a : b, right = left === a ? b : a;
    const top = ar.y < br.y ? a : b, bottom = top === a ? b : a;
    if (Math.abs(left.rect.x + left.rect.width - right.rect.x) <= 1 && Math.min(ar.y + ar.height, br.y + br.height) > Math.max(ar.y, br.y)) {
      axis = 'vertical'; start = Math.max(ar.y, br.y); length = Math.min(ar.y + ar.height, br.y + br.height) - start; coordinate = right.rect.x; target = left;
    } else if (Math.abs(top.rect.y + top.rect.height - bottom.rect.y) <= 1 && Math.min(ar.x + ar.width, br.x + br.width) > Math.max(ar.x, br.x)) {
      axis = 'horizontal'; start = Math.max(ar.x, br.x); length = Math.min(ar.x + ar.width, br.x + br.width) - start; coordinate = bottom.rect.y; target = top;
    } else continue;
    const handle = el('div', `resize-handle ${axis}`); handle.title = 'Drag to resize panes';
    if (axis === 'vertical') Object.assign(handle.style, { left: `${coordinate * cell.width - 6}px`, top: `${start * cell.height}px`, height: `${length * cell.height}px` });
    else Object.assign(handle.style, { top: `${coordinate * cell.height - 6}px`, left: `${start * cell.width}px`, width: `${length * cell.width}px` });
    handle.addEventListener('pointerdown', safely(async event => {
      document.body.dataset.resizing = 'true'; handle.setPointerCapture(event.pointerId);
      let last = axis === 'vertical' ? event.clientX : event.clientY;
      let queued = Promise.resolve();
      const ready = action({ type: 'focus_pane', pane: target.id });
      handle.onpointermove = e => {
        const point = axis === 'vertical' ? e.clientX : e.clientY;
        const cells = Math.trunc((point - last) / (axis === 'vertical' ? cell.width : cell.height));
        if (!cells) return;
        last += cells * (axis === 'vertical' ? cell.width : cell.height);
        queued = queued.then(() => ready).then(() => action({ type: 'resize_pane', dir: axis === 'vertical' ? 'right' : 'down', cells })).catch(() => {});
      };
      handle.onpointerup = handle.onpointercancel = () => { delete document.body.dataset.resizing; handle.onpointermove = null; scheduleRender(); };
    }));
    $('#panes').append(handle);
  }
}
async function loadFiles() {
  if (!state.fileRoot || !state.files) return;
  const generation = ++state.directoryGeneration;
  const listing = await request({ type: 'directory', path: state.fileRoot });
  if (generation !== state.directoryGeneration) return;
  state.fileEntries = listing.entries; state.directoryCache.set(state.fileRoot, listing.entries);
  $('#file-breadcrumb').textContent = listing.path; $('#file-breadcrumb').title = listing.path;
  renderFiles();
  if (listing.truncated) showError('The file list reached the configured tree entry limit.');
}
function renderFiles() {
  const root = $('#file-tree'), filter = $('#file-filter').value.toLowerCase(); root.replaceChildren();
  let total = 0;
  function append(entries, depth) {
    for (const entry of entries) {
      if (++total > 4000 || depth > 12) return;
      if (filter && !entry.name.toLowerCase().includes(filter) && !entry.directory) continue;
      const node = button('', entry.path, async () => {
        if (entry.directory) {
          if (state.expanded.has(entry.path)) state.expanded.delete(entry.path);
          else {
            if (!state.directoryCache.has(entry.path)) {
              const listing = await request({ type: 'directory', path: entry.path });
              state.directoryCache.set(entry.path, listing.entries);
            }
            state.expanded.add(entry.path);
          }
          renderFiles();
        } else { await action({ type: 'open_viewer', path: entry.path }); terminalFocus(); }
      }, `file-row ${entry.directory ? 'directory' : ''}`);
      node.style.paddingLeft = `${9 + depth * 13}px`;
      node.append(el('span', 'file-icon', entry.directory ? (state.expanded.has(entry.path) ? '▾' : '▸') : '▫'), el('span', 'file-name', entry.name));
      if (!entry.directory && entry.name.includes('.')) node.append(el('span', 'extension', entry.name.split('.').pop().toUpperCase()));
      root.append(node);
      if (entry.directory && state.expanded.has(entry.path)) append(state.directoryCache.get(entry.path) || [], depth + 1);
    }
  }
  append(state.fileEntries, 0);
  if (!root.children.length) root.append(el('p', 'notice', 'No matching files.'));
}
function toggleFiles() {
  state.files = !state.files; localStorage.setItem('files', state.files); $('#files').hidden = !state.files;
  $('#files-button').setAttribute('aria-pressed', state.files); if (state.files) return loadFiles();
}
async function toggleRich() {
  state.rich = !state.rich;
  await request({ type: 'rich_output', enabled: state.rich });
  localStorage.setItem('richOutput', state.rich);
  $('#rich-output-button').setAttribute('aria-pressed', state.rich);
}
async function addWorkspace() { const dir = await api.folder(); if (dir) await action({ type: 'new_workspace', dir }); }
function find() {
  const query = $('#find-input').value.toLowerCase(); let count = 0;
  for (const view of state.panes.values()) for (const row of view.lines.children) {
    const match = !!query && row.textContent.toLowerCase().includes(query);
    row.classList.toggle('match', match); if (match) count++;
  }
  $('#find-count').textContent = query ? `${count} matching ${count === 1 ? 'row' : 'rows'}` : '';
}
function openFind() { $('#find-bar').hidden = false; $('#find-input').focus(); }
function closeFind() { $('#find-bar').hidden = true; $('#find-input').value = ''; find(); terminalFocus(); }
function palette() {
  $('#palette').showModal(); $('#command-input').value = ''; state.paletteIndex = 0; renderCommands(); $('#command-input').focus();
}
function settings() {
  $('#theme-select').replaceChildren(...state.themes.map(name => { const option = el('option', '', name); option.value = name; return option; }));
  $('#theme-select').value = state.theme; $('#font-family').value = state.fontFamily; $('#settings').showModal();
}
const commands = [
  ['New terminal tab', '⌘ T', () => action({ type: 'new_tab' })],
  ['Split terminal right', '⌘ D', () => action({ type: 'split', dir: 'vertical' })],
  ['Split terminal down', '⌘ ⇧ D', () => action({ type: 'split', dir: 'horizontal' })],
  ['Zoom focused pane', '⌘ ⇧ ↵', () => action({ type: 'toggle_zoom' })],
  ['Focus next pane', '', () => action({ type: 'focus_cycle', step: 1 })],
  ['Add workspace folder', '', addWorkspace],
  ['Rename current tab', '', () => rename('tab')],
  ['Rename workspace', '', () => rename('workspace')],
  ['Find in visible terminal output', '⌘ F', openFind],
  ['Toggle file explorer', '⌘ B', toggleFiles],
  ['Toggle rich terminal output', '', toggleRich],
  ['Appearance and themes', '⌘ ,', settings],
  ['Open workspace folder tree', '', () => action({ type: 'open_viewer', path: workspace()?.dir || state.cwd, kind: 'tree' })],
  ['Scroll focused pane to top', '', () => focused() && action({ type: 'scroll_to_top', pane: focused().id })],
  ['Return focused pane to live output', '', () => focused() && action({ type: 'scroll_to_bottom', pane: focused().id })],
  ['Copy visible terminal output', '', () => navigator.clipboard.writeText(Terminal.plain(state.screens[focused()?.id] || []))],
  ['Close focused pane', '⌘ ⇧ W', () => action({ type: 'close_pane' })],
  ['Check for updates', '', () => { settings(); return checkUpdate(); }],
  ['Reconnect to session', '', () => api.reconnect()],
];
function renderCommands() {
  const query = $('#command-input').value.toLowerCase(); state.commands = commands.filter(([name]) => name.toLowerCase().includes(query));
  state.paletteIndex = Math.min(state.paletteIndex, Math.max(0, state.commands.length - 1));
  $('#command-list').replaceChildren();
  state.commands.forEach(([name, shortcut, run], index) => {
    const node = button('', name, async () => { $('#palette').close(); await run(); }, index === state.paletteIndex ? 'selected' : '');
    node.append(el('span', '', name)); if (shortcut) node.append(el('kbd', '', shortcut)); $('#command-list').append(node);
  });
}
async function rename(kind) {
  // A dedicated dialog keeps native terminal input outside browser prompt APIs.
  const dialog = el('dialog'), form = el('form'); form.method = 'dialog'; form.style.padding = '24px';
  const label = el('label', '', `Rename ${kind} `), input = el('input'); input.value = kind === 'tab' ? workspace().tabs[workspace().active_tab].name : workspace().name;
  input.style.borderBottom = '1px solid var(--border)'; label.append(input); form.append(label);
  const submit = el('button', 'primary', 'Rename'); submit.type = 'submit'; form.append(submit); dialog.append(form); document.body.append(dialog);
  dialog.addEventListener('close', safely(async () => { const name = input.value.trim(); if (dialog.returnValue !== 'cancel' && name) await action({ type: `rename_${kind}`, name }); dialog.remove(); }));
  dialog.addEventListener('cancel', () => { dialog.returnValue = 'cancel'; });
  dialog.showModal(); input.focus(); input.select();
}
function receive(event) {
  if (event.type === 'ready') {
    state.connected = true; state.cwd = event.cwd; state.theme = event.theme; state.themes = event.themes;
    applyPalette(event.palette); state.dimensions = ''; resize();
    if (!state.rich) safely(() => request({ type: 'rich_output', enabled: false }))();
    const saved = localStorage.getItem('theme'); if (saved && saved !== event.theme) safely(() => setTheme(saved))();
    $('#connection-status').textContent = 'Connected'; $('#connection-dot').style.background = 'var(--success)'; $('#banner').hidden = true;
  } else if (event.type === 'snapshot') {
    state.snapshot = event.snapshot; state.screens = event.screens; scheduleRender();
  } else if (['disconnected', 'detached'].includes(event.type)) {
    state.connected = false; $('#connection-status').textContent = 'Disconnected'; $('#connection-dot').style.background = 'var(--error)'; showError(event.message || event.reason);
  } else if (event.type === 'error') showError(event.message);
}
let terminalComposing = false, swallowInput = false;
on('#terminal-input', 'keydown', async event => {
  if (!state.connected || terminalComposing || event.isComposing) return;
  // Native copy/paste stays in the OS menu/clipboard path.
  if (event.metaKey) return;
  if (event.ctrlKey && event.shiftKey && ['C', 'V'].includes(event.key.toUpperCase())) {
    event.preventDefault();
    if (event.key.toUpperCase() === 'C') await navigator.clipboard.writeText(window.getSelection()?.toString() || '');
    else await action({ type: 'paste', text: await navigator.clipboard.readText() });
    return;
  }
  const input = Terminal.keyInput(event);
  if (!input) return;
  // Text enters via input events, including dead keys and IME composition.
  if (typeof input.key === 'object' && input.key.char && !event.ctrlKey && !event.altKey && !event.metaKey) return;
  event.preventDefault(); await action({ type: 'key', input });
});
$('#terminal-input').addEventListener('compositionstart', () => { terminalComposing = true; });
$('#terminal-input').addEventListener('compositionend', safely(async event => {
  terminalComposing = false; swallowInput = true;
  setTimeout(() => { swallowInput = false; }, 0);
  if (event.data) await action({ type: 'send_text', text: event.data, submit: false });
  $('#terminal-input').value = '';
}));
on('#terminal-input', 'input', async event => {
  if (terminalComposing || event.isComposing || swallowInput) return;
  const text = $('#terminal-input').value; $('#terminal-input').value = '';
  if (text) await action({ type: 'send_text', text, submit: false });
});
on('#terminal-input', 'paste', async event => {
  event.preventDefault(); await action({ type: 'paste', text: event.clipboardData.getData('text/plain') });
});
document.addEventListener('keydown', safely(async event => {
  const command = /Mac/.test(navigator.platform) ? event.metaKey : event.ctrlKey;
  if (event.key === 'Escape' && !$('#palette').open && !$('#settings').open && !$('#find-bar').hidden) { closeFind(); return; }
  if (!command) return;
  const key = event.key.toLowerCase();
  const shortcuts = {
    p: event.shiftKey ? palette : null, t: () => action({ type: 'new_tab' }),
    d: () => action({ type: 'split', dir: event.shiftKey ? 'horizontal' : 'vertical' }),
    b: toggleFiles, f: openFind, ',': settings,
    enter: event.shiftKey ? () => action({ type: 'toggle_zoom' }) : null,
    w: event.shiftKey ? () => action({ type: 'close_pane' }) : null,
    '=': () => fontSize(1), '+': () => fontSize(1), '-': () => fontSize(-1),
  };
  if (/^[1-9]$/.test(key)) { event.preventDefault(); event.stopPropagation(); await action({ type: 'select_tab', index: Number(key) - 1 }); return; }
  if (shortcuts[key]) { event.preventDefault(); event.stopPropagation(); await shortcuts[key](); }
}), true);
on('#palette-button', 'click', palette); on('#status-commands', 'click', palette);
on('#new-tab', 'click', () => action({ type: 'new_tab' })); on('#add-workspace', 'click', addWorkspace);
on('#split-right', 'click', () => action({ type: 'split', dir: 'vertical' }));
on('#split-down', 'click', () => action({ type: 'split', dir: 'horizontal' }));
on('#zoom-button', 'click', () => action({ type: 'toggle_zoom' }));
on('#files-button', 'click', toggleFiles); on('#refresh-files', 'click', () => { state.directoryCache.clear(); state.expanded.clear(); return loadFiles(); });
on('#file-filter', 'input', renderFiles); on('#rich-output-button', 'click', toggleRich);
on('#find-button', 'click', openFind); on('#find-input', 'input', find); on('#find-close', 'click', closeFind);
on('#settings-button', 'click', settings); on('#close-settings', 'click', () => $('#settings').close());
on('#theme-select', 'change', event => setTheme(event.target.value));
on('#font-family', 'change', event => { state.fontFamily = event.target.value; localStorage.setItem('fontFamily', state.fontFamily); fontSize(); });
on('#font-smaller', 'click', () => fontSize(-1)); on('#font-larger', 'click', () => fontSize(1));
on('#reconnect', 'click', () => api.reconnect()); on('#dismiss-banner', 'click', () => { $('#banner').hidden = true; });
on('#command-input', 'input', () => { state.paletteIndex = 0; renderCommands(); });
on('#command-input', 'keydown', async event => {
  if (['ArrowDown', 'ArrowUp'].includes(event.key)) {
    event.preventDefault(); const length = state.commands.length;
    if (length) state.paletteIndex = (state.paletteIndex + (event.key === 'ArrowDown' ? 1 : -1) + length) % length;
    renderCommands(); $('#command-list .selected')?.scrollIntoView({ block: 'nearest' });
  } else if (event.key === 'Enter' && state.commands[state.paletteIndex]) {
    event.preventDefault(); $('#palette').close(); await state.commands[state.paletteIndex][2]();
  }
});
api.onEvent(receive); api.onCommand(command => { if (command === 'palette' && !$('#palette').open) palette(); });
$('#files').hidden = !state.files; $('#files-button').setAttribute('aria-pressed', state.files);
$('#rich-output-button').setAttribute('aria-pressed', state.rich);
safely(async () => {
  // PTY geometry must use the loaded font, not a temporary system fallback.
  try { await document.fonts.load(`${state.fontSize}px "Herdstead Mono"`); }
  catch { showError('The bundled terminal font could not load. Using a system font.'); }
  fontSize();
  for (const event of await api.state()) receive(event);
  terminalFocus();
})();

async function checkUpdate() {
  const button = $('#check-update');
  button.disabled = true;
  $('#install-update').hidden = true;
  $('#update-status').textContent = 'Checking for updates…';
  try {
    if (!api.checkUpdate) throw new Error('Updates are available in the installed desktop app.');
    const update = await api.checkUpdate();
    $('#update-status').textContent = update ? `Version ${update.version} is available.` : 'You are up to date.';
    $('#install-update').hidden = !update;
  } catch (error) {
    $('#update-status').textContent = `Cannot check for updates: ${error.message || error}`;
  } finally { button.disabled = false; }
}
on('#check-update', 'click', checkUpdate);
on('#install-update', 'click', async () => {
  $('#install-update').disabled = true;
  $('#check-update').disabled = true;
  $('#update-status').textContent = 'Downloading and installing update…';
  try { await api.installUpdate(); }
  catch (error) {
    $('#update-status').textContent = `Update failed: ${error.message || error}. Check for updates to try again.`;
    $('#install-update').hidden = true;
    $('#install-update').disabled = false;
    $('#check-update').disabled = false;
  }
});
