// Scripts use CDP over local Chrome only. Blocking arbitrary outbound
// fetch/WebSocket keeps generated examples from becoming network clients;
// use browser-discovered API replay outside this runner when needed.
// Deliberate exception: sourcemap-resolver.mjs fetches .map files via
// Node's http/https core modules (not globalThis.fetch), which this
// override does not touch — required since source maps live at the
// page's own real domain, not localhost.

function isLocalhost(url: unknown) {
  try {
    const h = new URL(String(url)).hostname;
    return h === 'localhost' || h === '127.0.0.1' || h === '::1';
  } catch {
    return false;
  }
}

/** Replaces globalThis.fetch and WebSocket with versions that refuse non-localhost URLs. */
export function restrictNetworkToLocalhost() {
  const origFetch = globalThis.fetch;
  const OrigWS = globalThis.WebSocket;
  globalThis.fetch = function restrictedFetch(input, init) {
    const url =
      typeof input === 'string'
        ? input
        : input instanceof URL
          ? input.href
          : (input?.url ?? '');
    if (!isLocalhost(url)) {
      throw new Error(
        `[SANDBOX] fetch blocked: only localhost allowed (attempted: ${url})`
      );
    }
    return origFetch(input, init);
  };
  globalThis.WebSocket = class RestrictedWebSocket extends OrigWS {
    constructor(url: string | URL, ...args: [protocols?: string | string[]]) {
      if (!isLocalhost(url)) {
        throw new Error(
          `[SANDBOX] WebSocket blocked: only localhost allowed (attempted: ${url})`
        );
      }
      super(url, ...args);
    }
  };
}
