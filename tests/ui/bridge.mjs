// Simulate the IPC transport and native close event while exercising the real
// @tauri-apps/api window/event wrappers. This does not launch a native WebView.
export async function nativeBridge(page, handler) {
  await page.exposeFunction('nativeBackend', handler);
  await page.addInitScript(() => {
    globalThis.isTauri = true;
    const callbacks = new Map(); let nextId = 1; let closeHandler;
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback(callback) { const id = nextId++; callbacks.set(id, callback); return id; },
      invoke: async (command, args) => {
        if (command === 'plugin:event|listen') { if (args.event === 'tauri://close-requested') closeHandler = callbacks.get(args.handler); return nextId++; }
        if (command === 'plugin:event|unlisten') return;
        const result = await window.nativeBackend(command, args);
        return command === 'read_current_pdf' ? new Uint8Array(result).buffer : result;
      },
    };
    window.requestNativeClose = () => closeHandler({ event: 'tauri://close-requested', id: 1, payload: null });
  });
}
