// Scripts use CDP over local Chrome only. The guard refuses non-loopback
// fetch, WebSocket, net.Socket.connect (so node:http, node:https, node:tls and
// ws clients too), dns lookups/resolves and dgram send/connect.
// NOT a complete boundary: child_process, worker threads that load their own
// copies, native addons, and in-process opt-outs are not covered.
// Deliberate, narrow exception: sourcemap-resolver.mjs fetches .map files via
// node:http(s) from the page's own real domain. It wraps that one call in
// the scope found at globalThis[Symbol.for(EXEMPT_KEY)], which this module
// installs. A script can use the same scope, so this is a guard against
// generated code becoming a network client, not against hostile code.

import dgram from 'node:dgram';
import dns from 'node:dns';
import { syncBuiltinESMExports } from 'node:module';
import net from 'node:net';
import { AsyncLocalStorage } from 'node:async_hooks';

export const EXEMPT_KEY = 'octocode.chrome-devtools.sandbox-exempt';
const exempt = new AsyncLocalStorage<boolean>();
(globalThis as Record<symbol, unknown>)[Symbol.for(EXEMPT_KEY)] = exempt;

/** True for a loopback hostname or address; a name is judged as written, never resolved. */
export function isLoopbackHost(host: unknown) {
  if (typeof host !== 'string') return false;
  const h = host.replace(/^\[|\]$/g, '').toLowerCase();
  return (
    h === 'localhost' ||
    h === '::1' ||
    /^127(\.\d{1,3}){3}$/.test(h) ||
    /^::ffff:127(\.\d{1,3}){3}$/.test(h)
  );
}

function refuse(what: string, target: unknown): never {
  throw new Error(
    `[SANDBOX] ${what} blocked: only loopback allowed (attempted: ${String(target)})`
  );
}

function checkHost(what: string, host: unknown) {
  if (exempt.getStore()) return;
  if (!isLoopbackHost(host)) refuse(what, host);
}

/** The host a net.Socket.connect call targets; null for a unix-socket path. */
function connectHost(args: unknown[]): string | null {
  let [first, second] = args;
  // net.connect, net.createConnection and node:http call socket.connect([options, callback]).
  if (Array.isArray(first)) [first, second] = first;
  if (first && typeof first === 'object') {
    const o = first as { path?: string; host?: string; lookup?: unknown };
    if (o.path) return null;
    // A custom lookup picks the address after this check, so the name alone proves nothing.
    if (o.lookup && !exempt.getStore()) refuse('net', 'custom lookup');
    return o.host ?? 'localhost';
  }
  if (typeof first === 'string' && Number.isNaN(Number(first))) return null;
  return typeof second === 'string' ? second : 'localhost';
}

function isLocalhost(url: unknown) {
  try {
    const h = new URL(String(url)).hostname;
    return h === 'localhost' || h === '127.0.0.1' || h === '::1';
  } catch {
    return false;
  }
}

/** Refuses non-loopback fetch, WebSocket, TCP, DNS and UDP use; see the file header for what stays open. */
export function restrictNetworkToLocalhost() {
  restrictNodeNetwork();
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

function restrictNodeNetwork() {
  const origConnect = net.Socket.prototype.connect as (
    ...a: unknown[]
  ) => unknown;
  net.Socket.prototype.connect = function restrictedConnect(
    this: net.Socket,
    ...args: unknown[]
  ) {
    const host = connectHost(args);
    if (host !== null) checkHost('net', host);
    return origConnect.apply(this, args);
  } as typeof net.Socket.prototype.connect;

  const dnsTarget = dns as unknown as Record<string, unknown>;
  const promisesTarget = dns.promises as unknown as Record<string, unknown>;
  const wrapDns = (target: Record<string, unknown>, name: string) => {
    const orig = target[name] as (...a: unknown[]) => unknown;
    target[name] = function restrictedDns(this: unknown, ...args: unknown[]) {
      // lookupService takes an address, the rest a hostname: both first.
      checkHost('dns', args[0]);
      return orig.apply(this, args);
    };
  };
  for (const target of [dnsTarget, promisesTarget])
    for (const name of Object.keys(target))
      if (
        typeof target[name] === 'function' &&
        /^(lookup|lookupService|resolve\w*|reverse)$/.test(name)
      )
        wrapDns(target, name);

  const sockProto = dgram.Socket.prototype as unknown as Record<
    string,
    (...a: unknown[]) => unknown
  >;
  for (const name of ['send', 'connect']) {
    const orig = sockProto[name];
    sockProto[name] = function restrictedDgram(
      this: unknown,
      ...args: unknown[]
    ) {
      // send(msg, [offset, length,] port, address): msg may be a string, so
      // skip it; a missing address means the loopback default or a connected peer.
      const rest = name === 'send' ? args.slice(1) : args;
      for (const a of rest) if (typeof a === 'string') checkHost('dgram', a);
      return orig.apply(this, args);
    };
  }
  syncBuiltinESMExports();
}
