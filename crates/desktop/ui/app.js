/* global Terminal */
const $ = selector => document.querySelector(selector);
const api = window.herdstead;
const state = {
  snapshot: null, screens: {}, connected: false, theme: '', themes: [], cwd: '', home: '',
  fontSize: Number(localStorage.getItem('fontSize')) || 14,
  files: localStorage.getItem('files') !== 'false', rich: localStorage.getItem('richOutput') !== 'false',
  fontFamily: localStorage.getItem('fontFamily') || '',
  panes: new Map(), fileRoot: '', fileEntries: [], directoryCache: new Map(),
  expanded: new Set(), paletteIndex: 0, commands: [], cell: { width: 8.4, height: 21 },
  dimensions: '', renderPending: false, directoryGeneration: 0, viewerCache: new Map(),
  zoom: clampZoom(Number(localStorage.getItem('zoom')) || 1),
  workspaceNodes: new Map(), tabNodes: new Map(), agentNodes: new Map(),
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
function on(selector, name, fn) {
  $(selector).addEventListener(name, safely(async (...args) => { await fn(...args); }));
}
// A press acts straight away instead of waiting for the release, so a click that
// a drag region, a re-render or the webview drops still lands on the first
// press. The click that follows a press is ignored, keeping toggles to one
// action; keyboard activation has no press, so its click still runs. `repeat`
// re-runs a handler on the click, for openers whose focus the press can lose to
// the browser's own mousedown default.
function press(node, fn, { repeat = false } = {}) {
  let pressed = 0, release = null;
  const run = safely(fn);
  const drop = () => { if (release) node.removeEventListener('pointerup', release); release = null; };
  node.addEventListener('pointerdown', event => {
    if (event.button !== 0 || event.ctrlKey) return;
    pressed = performance.now();
    run(event);
    // Give the caret back on release even when no click follows the press.
    drop();
    release = () => { drop(); restoreTerminalFocus(); };
    node.addEventListener('pointerup', release);
  });
  node.addEventListener('pointercancel', drop);
  node.addEventListener('click', safely(async event => {
    drop();
    if (repeat || performance.now() - pressed > 600) await run(event);
    restoreTerminalFocus();
  }));
  return node;
}
function onPress(selector, fn, options) { press($(selector), fn, options); }
function button(text, title, fn, className = '') {
  const node = document.createElement('button');
  node.textContent = text; node.title = title; node.className = className;
  return press(node, event => fn(event));
}
function el(tag, className, text) {
  const node = document.createElement(tag); if (className) node.className = className;
  if (text !== undefined) node.textContent = text; return node;
}
// Wraps the filter's matched substring in <mark> so a hit is visible, not just present.
function fileNameNode(name, filter) {
  const node = el('span', 'file-name');
  if (!filter) { node.textContent = name; return node; }
  const index = name.toLowerCase().indexOf(filter);
  if (index === -1) { node.textContent = name; return node; }
  node.append(name.slice(0, index), el('mark', '', name.slice(index, index + filter.length)), name.slice(index + filter.length));
  return node;
}
function setText(node, text) { if (node.textContent !== text) node.textContent = text; }
// Rebuilds a button list by stable key instead of tearing it down: a node a
// pointer is mid-click on survives a snapshot update that changes something
// else, so clicks never need a second try while the terminal is busy. Moving
// a node that is already correctly placed would itself detach and reattach
// it, which is enough to drop an in-flight click, so it is only repositioned
// when the order actually changed.
function syncList(container, items, keyFn, cache, buildFn, updateFn) {
  const seen = new Set();
  let previous = null;
  for (const item of items) {
    const key = keyFn(item);
    seen.add(key);
    let node = cache.get(key);
    if (!node) { node = buildFn(item); cache.set(key, node); }
    updateFn(node, item);
    const inPlace = previous ? node.previousElementSibling === previous : node === container.firstElementChild;
    if (!inPlace) { if (previous) previous.after(node); else container.prepend(node); }
    previous = node;
  }
  for (const [key, node] of cache) if (!seen.has(key)) { node.remove(); cache.delete(key); }
}
function uiIcon(kind) {
  const paths = {
    right: 'M4 5h16v14H4z M12 5v14',
    down: 'M4 5h16v14H4z M4 12h16',
    zoom: 'M14 4h6v6 M20 4l-7 7 M10 20H4v-6 M4 20l7-7',
    close: 'M6 6l12 12 M18 6L6 18',
    search: 'M15.5 15.5L21 21 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',
    settings: 'M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1z M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0',
    chevronRight: 'M9 5l7 7-7 7',
    chevronDown: 'M5 9l7 7 7-7',
    file: 'M5 3h9l5 5v13H5z M14 3v6h5 M8 13h6 M8 16h6',
    files: 'M4 3h16v18H4z M8 7h8 M8 11h8 M8 15h8',
    refresh: 'M20 8a9 9 0 1 0 1 7 M20 3v6h-6',
    sparkle: 'M12 2l3 7 7 3-7 3-3 7-3-7-7-3 7-3z',
    computer: 'M3 4h18v13H3z M8 21h8 M12 17v4',
    command: 'M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z',
    folder: 'M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2.4 3h8.1A1.5 1.5 0 0 1 21 10.5v7A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z M3 10h18',
  };
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true'); svg.classList.add('ui-icon');
  const path = document.createElementNS(svg.namespaceURI, 'path');
  path.setAttribute('d', paths[kind]); svg.append(path); return svg;
}
function paneIcon(button, kind) { button.replaceChildren(uiIcon(kind)); }
document.querySelectorAll('[data-icon]').forEach(node => node.replaceWith(uiIcon(node.dataset.icon)));
function showError(message) {
  $('#banner span').textContent = message; $('#banner').hidden = false;
}
function focused() { return state.snapshot?.panes.find(p => p.id === state.snapshot.focused); }
function workspace() { return state.snapshot?.workspaces[state.snapshot.active_workspace]; }
function displayPath(path) {
  const home = state.home.replace(/\/$/, '');
  return home && (path === home || path.startsWith(`${home}/`)) ? `~${path.slice(home.length)}` : path;
}
function workspaceName(item) {
  return displayPath(item.dir) === '~' && item.name === item.dir.split('/').pop() ? '~' : item.name;
}
function agentLabel(agent) { return typeof agent === 'string' ? agent === 'open_code' ? 'opencode' : agent : agent?.other || ''; }
function terminalFocus() { if (!document.querySelector('dialog[open]')) $('#terminal-input').focus({ preventScroll: true }); }
function restoreTerminalFocus() {
  const active = document.activeElement;
  if (active === document.body || active?.tagName === 'BUTTON') terminalFocus();
}
function updateInputFocus() {
  document.body.classList.toggle('terminal-focused', document.hasFocus() && document.activeElement === $('#terminal-input'));
}
document.addEventListener('focusin', updateInputFocus);
document.addEventListener('focusout', () => queueMicrotask(updateInputFocus));
window.addEventListener('blur', updateInputFocus);
window.addEventListener('focus', () => { restoreTerminalFocus(); updateInputFocus(); });
function colorStatus(status) { return status === 'blocked' ? 'var(--warning)' : status === 'exited' ? 'var(--error)' : status === 'idle' ? 'var(--faint)' : 'var(--accent)'; }
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
function clampZoom(value) {
  const zoom = Math.round(Number(value) * 10) / 10;
  if (!Number.isFinite(zoom)) return 1;
  return Math.min(1.6, Math.max(0.7, zoom));
}
function setZoom(value) {
  state.zoom = clampZoom(value);
  localStorage.setItem('zoom', String(state.zoom));
  // The webview reports no zoom level, so the window keeps the value and the
  // native side only applies it.
  Promise.resolve(api.setZoom?.(state.zoom)).catch(error => showError(error.message));
  requestAnimationFrame(() => { resize(); scheduleRender(); });
}
function zoomText(step) { setZoom(step === 0 ? 1 : state.zoom + step * 0.1); }
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
  $('#session-name').textContent = snap.session === ws.name ? workspaceName(ws) : snap.session;
  $('#dimensions').textContent = focused()?.grid ? `${focused().grid.cols} × ${focused().grid.rows} · UTF-8` : 'Rich viewer';
  // Each node keeps its current index/target in a dataset attribute, read at
  // click time: the handler closes over the stable `node`, not a snapshot
  // object that a later render replaces.
  setText($('#space-count'), String(snap.workspaces.length));
  syncList($('#workspaces'), snap.workspaces, item => item.id, state.workspaceNodes, () => {
    let node;
    node = button('', '', () => action({ type: 'select_workspace', index: Number(node.dataset.index) }), 'workspace');
    node.append(el('span', 'initial'), el('span', 'name'), el('span', 'dot'));
    return node;
  }, (node, item) => {
    const index = snap.workspaces.indexOf(item), name = workspaceName(item);
    node.dataset.index = index;
    node.dataset.status = item.status || 'idle';
    node.title = item.dir;
    node.className = `workspace ${index === snap.active_workspace ? 'active' : ''}`;
    setText(node.querySelector('.initial'), name.slice(0, 1).toUpperCase());
    setText(node.querySelector('.name'), name);
    node.querySelector('.dot').style.background = colorStatus(item.status);
  });
  syncList($('#tabs'), ws.tabs, tab => tab.id, state.tabNodes, () => {
    let node;
    node = button('', '', () => action({ type: 'select_tab', index: Number(node.dataset.index) }), 'tab');
    const close = el('span', 'close-tab', '×');
    close.addEventListener('click', safely(async event => {
      event.stopPropagation();
      const index = Number(node.dataset.index);
      await action({ type: 'select_tab', index }); await action({ type: 'close_tab' }); terminalFocus();
    }));
    node.append(el('span', 'prompt', '❯_'), el('span', 'name'), close);
    return node;
  }, (node, tab) => {
    const index = ws.tabs.indexOf(tab);
    node.dataset.index = index;
    node.title = `Switch to ${tab.name}`;
    node.className = `tab ${index === ws.active_tab ? 'active' : ''}`;
    setText(node.querySelector('.name'), tab.name);
    node.querySelector('.close-tab').title = `Close ${tab.name}`;
  });
  const agents = snap.workspaces.flatMap((item, index) => item.tabs.filter(tab => tab.agent).map(tab => ({ ...tab, agent: agentLabel(tab.agent), workspaceIndex: index, workspaceName: workspaceName(item), tabIndex: item.tabs.indexOf(tab) })));
  $('#agent-count').textContent = agents.length;
  const agentsList = $('#agents');
  let empty = agentsList.querySelector('.empty');
  if (!agents.length) {
    if (!empty) { empty = el('p', 'empty', 'Agents appear here when they start working in a terminal. Your herd keeps running when you close the window.'); agentsList.append(empty); }
  } else empty?.remove();
  syncList(agentsList, agents, agent => agent.id, state.agentNodes, () => {
    let node;
    node = button('', '', async () => {
      await action({ type: 'select_workspace', index: Number(node.dataset.workspaceIndex) });
      await action({ type: 'select_tab', index: Number(node.dataset.tabIndex) });
    }, 'agent');
    const info = el('span', 'agent-info'); info.append(el('strong'), el('small'));
    node.append(el('span', 'agent-icon', '✳'), info, el('span', 'dot'));
    return node;
  }, (node, agent) => {
    node.dataset.workspaceIndex = agent.workspaceIndex;
    node.dataset.tabIndex = agent.tabIndex;
    node.title = `Open ${agent.agent}`;
    setText(node.querySelector('strong'), agent.agent);
    setText(node.querySelector('small'), `${agent.workspaceName} · ${agent.status}`);
    node.querySelector('.dot').style.background = colorStatus(agent.status);
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
  const signature = JSON.stringify([state.snapshot.session, state.snapshot.active_workspace, state.snapshot.workspaces, state.snapshot.focused, state.snapshot.panes.map(p => [p.id, p.title, p.status]), state.snapshot.zoomed]);
  if (signature !== chromeSignature) { chromeSignature = signature; renderChrome(); }
  setText($('#dimensions'), focused()?.grid ? `${focused().grid.cols} × ${focused().grid.rows} · UTF-8` : 'Rich viewer');
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
      const lines = el('div', 'rows');
      screen.append(lines); node.append(header, screen);
      node.addEventListener('mousedown', event => {
        if (event.target.closest('button')) return;
        // The textarea takes the keystrokes immediately; the daemon catches up
        // with the focus change before it reads any of them.
        terminalFocus();
        safely(async () => {
          if (state.snapshot.focused !== pane.id) await action({ type: 'focus_pane', pane: pane.id });
        })();
      });
      node.addEventListener('mouseup', () => { if (!window.getSelection()?.toString()) terminalFocus(); });
      node.addEventListener('wheel', safely(event => {
        if (pane.kind === 'viewer') return;
        event.preventDefault();
        if (event.deltaY) queueScroll(pane.id, event.deltaY < 0 ? 3 : -3);
      }), { passive: false });
      $('#panes').append(node); view = { node, title, cwd, offset, screen, lines, zoom }; state.panes.set(pane.id, view);
    }
    const cell = state.cell, rect = pane.rect;
    const geometry = JSON.stringify([rect, snap.area, cell]);
    if (geometry !== view.geometry) {
      view.geometry = geometry;
      Object.assign(view.node.style, { left: `${(rect.x - snap.area.x) * cell.width}px`, top: `${(rect.y - snap.area.y) * cell.height}px`, width: `${rect.width * cell.width}px`, height: `${rect.height * cell.height}px` });
    }
    const metadata = JSON.stringify([pane.title, pane.cwd, pane.grid?.offset, pane.exit_code, pane.id === snap.focused, snap.zoomed]);
    if (metadata !== view.metadata) {
      view.metadata = metadata;
      view.node.classList.toggle('focused', pane.id === snap.focused);
      view.zoom.setAttribute('aria-pressed', snap.zoomed && pane.id === snap.focused);
      view.zoom.title = snap.zoomed ? 'Restore pane layout' : 'Zoom pane';
      view.zoom.setAttribute('aria-label', view.zoom.title);
      setText(view.title, pane.title);
      setText(view.cwd, displayPath(pane.cwd)); view.cwd.title = pane.cwd;
      setText(view.offset, pane.grid?.offset ? `↑ ${pane.grid.offset} · click for live` : pane.exit_code !== null ? `exit ${pane.exit_code}` : '');
    }
    view.offset.onclick = () => safely(() => action({ type: 'scroll_to_bottom', pane: pane.id }))();
    const onLink = link => safely(() => openLink(link, pane.cwd))();
    if (pane.grid) {
      Terminal.updateRows(view.lines, state.screens[pane.id] || [], document, onLink);
    } else safely(() => renderViewer(pane, view))();
  }
  renderDividers(); find();
}
const pendingScroll = new Map();
let scrollPending = false;
function queueScroll(pane, delta) {
  pendingScroll.set(pane, (pendingScroll.get(pane) || 0) + delta);
  if (scrollPending) return;
  scrollPending = true;
  requestAnimationFrame(() => {
    scrollPending = false;
    for (const [pane, delta] of pendingScroll) {
      if (delta && state.panes.has(pane)) safely(() => action({ type: 'scroll_pane', pane, delta, page: false }))();
    }
    pendingScroll.clear();
  });
}
async function renderViewer(pane, view) {
  const width = Math.max(4, pane.rect.width - 2), key = `${pane.viewer?.path}:${width}:${state.theme}`;
  let cached = state.viewerCache.get(pane.id);
  if (cached?.pending || (cached?.key === key && Date.now() - cached.time < 1000)) return;
  cached = { key, pending: true, time: Date.now() }; state.viewerCache.set(pane.id, cached);
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
let dividerSignature = '';
function renderDividers() {
  // Keep handles stable during pointer capture.
  if (document.body.dataset.resizing) return;
  const signature = JSON.stringify([state.snapshot.panes.map(pane => [pane.id, pane.rect]), state.cell]);
  if (signature === dividerSignature) return;
  dividerSignature = signature;
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
  $('#file-breadcrumb').textContent = displayPath(listing.path).split('/').pop() || '/'; $('#file-breadcrumb').title = listing.path;
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
      const icon = el('span', 'file-icon');
      icon.append(uiIcon(entry.directory ? (state.expanded.has(entry.path) ? 'chevronDown' : 'chevronRight') : 'file'));
      if (entry.directory) icon.append(uiIcon('folder'));
      if (entry.directory) node.setAttribute('aria-expanded', state.expanded.has(entry.path));
      node.append(icon, fileNameNode(entry.name, filter));
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
  $('#rich-output-button').textContent = state.rich ? 'Enabled' : 'Disabled';
}
async function addWorkspace() { const dir = await api.folder(); if (dir) await action({ type: 'new_workspace', dir }); }
let findQuery = '';
function find() {
  const query = $('#find-input').value.toLowerCase(); let count = 0;
  if (!query && !findQuery) return;
  findQuery = query;
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
  if ($('#palette').open) $('#palette').close();
  $('#theme-select').replaceChildren(...state.themes.map(name => { const option = el('option', '', name); option.value = name; return option; }));
  $('#theme-select').value = state.theme; $('#font-family').value = state.fontFamily;
  if (!$('#settings').open) $('#settings').showModal();
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
  ['Zoom in', '⌘ +', () => zoomText(1)],
  ['Zoom out', '⌘ −', () => zoomText(-1)],
  ['Actual size', '⌘ 0', () => zoomText(0)],
  ['Toggle full screen', '⌃⌘ F', () => api.toggleFullScreen?.()],
  ['Settings and appearance', '⌘ ,', settings],
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
  const dialog = el('dialog'), form = el('form', 'rename'); form.method = 'dialog';
  const label = el('label', '', `Rename ${kind} `), input = el('input'); input.value = kind === 'tab' ? workspace().tabs[workspace().active_tab].name : workspace().name;
  label.append(input); form.append(label);
  const submit = el('button', 'primary', 'Rename'); submit.type = 'submit'; form.append(submit); dialog.append(form); document.body.append(dialog);
  dialog.addEventListener('close', safely(async () => { const name = input.value.trim(); if (dialog.returnValue !== 'cancel' && name) await action({ type: `rename_${kind}`, name }); dialog.remove(); }));
  dialog.addEventListener('cancel', () => { dialog.returnValue = 'cancel'; });
  dialog.addEventListener('close', restoreTerminalFocus);
  dialog.showModal(); input.focus(); input.select();
}
function receive(event) {
  if (event.type === 'ready') {
    state.connected = true; state.cwd = event.cwd; state.home = event.home || ''; state.theme = event.theme; state.themes = event.themes;
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
    '=': () => zoomText(1), '+': () => zoomText(1), '-': () => zoomText(-1), '0': () => zoomText(0),
  };
  if (/^[1-9]$/.test(key)) { event.preventDefault(); event.stopPropagation(); await action({ type: 'select_tab', index: Number(key) - 1 }); return; }
  if (shortcuts[key]) { event.preventDefault(); event.stopPropagation(); await shortcuts[key](); }
}), true);
// Openers re-run on the click so the input they focus keeps the caret.
onPress('#palette-button', palette, { repeat: true }); onPress('#status-commands', palette, { repeat: true });
onPress('#new-tab', () => action({ type: 'new_tab' })); onPress('#add-workspace', addWorkspace);
onPress('#files-button', toggleFiles); onPress('#refresh-files', () => { state.directoryCache.clear(); state.expanded.clear(); return loadFiles(); });
on('#file-filter', 'input', renderFiles); onPress('#rich-output-button', toggleRich);
onPress('#find-button', openFind, { repeat: true }); on('#find-input', 'input', find); onPress('#find-close', closeFind);
onPress('#settings-button', settings, { repeat: true }); onPress('#close-settings', () => $('#settings').close());
for (const dialog of ['#palette', '#settings']) on(dialog, 'close', restoreTerminalFocus);
on('#theme-select', 'change', event => setTheme(event.target.value));
on('#font-family', 'change', event => { state.fontFamily = event.target.value; localStorage.setItem('fontFamily', state.fontFamily); fontSize(); });
onPress('#font-smaller', () => fontSize(-1)); onPress('#font-larger', () => fontSize(1));
onPress('#reconnect', () => api.reconnect()); onPress('#dismiss-banner', () => { $('#banner').hidden = true; });
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
api.onEvent(receive); api.onCommand(safely(command => {
  if (command === 'palette' && !$('#palette').open) palette();
  else if (command === 'settings') settings();
  else if (command === 'zoom-in') zoomText(1);
  else if (command === 'zoom-out') zoomText(-1);
  else if (command === 'zoom-reset') zoomText(0);
  else if (command === 'check-for-updates') { settings(); if (!$('#check-update').disabled) return checkUpdate(); }
}));
$('#files').hidden = !state.files; $('#files-button').setAttribute('aria-pressed', state.files);
$('#rich-output-button').setAttribute('aria-pressed', state.rich);
$('#rich-output-button').textContent = state.rich ? 'Enabled' : 'Disabled';
safely(async () => {
  // PTY geometry must use the loaded font, not a temporary system fallback.
  try { await document.fonts.load(`${state.fontSize}px "Herdstead Mono"`); }
  catch { showError('The bundled terminal font could not load. Using a system font.'); }
  fontSize();
  if (api.setZoom) Promise.resolve(api.setZoom(state.zoom)).catch(error => showError(error.message));
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
onPress('#check-update', checkUpdate);
onPress('#install-update', async () => {
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
