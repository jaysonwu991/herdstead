const { invoke } = window.__TAURI__.core;
window.herdstead = {
  request: message => invoke('bridge_request', { message }),
  state: () => invoke('bridge_state'),
  reconnect: () => invoke('bridge_reconnect'),
  folder: () => invoke('choose_folder'),
  external: target => invoke('open_external', { target }),
  onEvent: callback => {
    const ready = window.__TAURI__.event.listen('herdstead:event', event => callback(event.payload));
    return () => ready.then(unlisten => unlisten());
  },
  onCommand: () => {},
};
// Tauri delegates the draggable title bar to the native window.
for (const selector of ['.brand', '#topbar']) document.querySelector(selector)?.addEventListener('mousedown', event => {
  if (event.button === 0 && !event.target.closest('button, #tabs')) window.__TAURI__.window.getCurrentWindow().startDragging();
});
