// Today's frontend, served by Vite from the repository's own vite.config.mjs,
// with the Tauri bridge stub (stub/bridge.js) loaded into index.html ahead of
// the app. Nothing of Tauri runs: every call the app makes is answered by the
// stub in the page.
//
// Two changes to the page, both here: the stub's script comes first in <head>,
// and the Google Fonts links go, so no run depends on the network or on a font
// that arrived in time.

import { join } from 'node:path';

export const STUB_ENTRY = '/scripts/ui-walk/stub/bridge.js';

function bridgeStub() {
  return {
    name: 'argentum-ui-walk-bridge-stub',
    transformIndexHtml: {
      order: 'pre',
      handler: (html) => ({
        html: html.replace(/\s*<link[^>]*fonts\.(googleapis|gstatic)\.com[^>]*>/g, ''),
        tags: [{ tag: 'script', attrs: { type: 'module', src: STUB_ENTRY }, injectTo: 'head-prepend' }],
      }),
    },
  };
}

/** Serve the frontend at a free local port. Returns { url, close }. */
export async function startServer(root) {
  // Imported here, so the walk's tests (scripts/ui-walk/test-ui-walk.mjs) run without node_modules.
  const { createServer } = await import('vite');
  const server = await createServer({
    root,
    configFile: join(root, 'vite.config.mjs'),
    // Its own cache, so the dev server's pre-bundled dependencies are never rewritten.
    cacheDir: join(root, 'node_modules', '.vite-ui-walk'),
    logLevel: 'error',
    clearScreen: false,
    // The page's console is read by the walk itself, not echoed by Vite.
    server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false, watch: null, forwardConsole: false },
    plugins: [bridgeStub()],
  });
  await server.listen();
  const { port } = server.httpServer.address();
  return { url: `http://127.0.0.1:${port}/`, close: () => server.close() };
}
