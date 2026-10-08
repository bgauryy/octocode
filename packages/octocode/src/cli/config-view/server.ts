import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from 'node:http';
import type { AddressInfo } from 'node:net';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import indexHtml from './assets/index.html?raw';
import appJs from './assets/app.js?raw';
import styleCss from './assets/style.css?raw';

export type ConfigViewRequest = (
  payload: Record<string, unknown>,
  context: { signal: AbortSignal }
) => Promise<unknown>;

export interface ConfigViewOptions {
  request: ConfigViewRequest;
  open?: (url: string) => Promise<void> | void;
  idleTimeoutMs?: number;
  signal?: AbortSignal;
  onReady?: (view: ConfigView) => Promise<void> | void;
}

interface ConfigView {
  origin: string;
  /** One-use bootstrap URL. Do not log it in telemetry. */
  url: string;
  close: () => Promise<void>;
  closed: Promise<void>;
}

type HttpFailure = Error & { status: number; code: string };

const OPERATIONS = new Set([
  'inspect',
  'agents',
  'setEnv',
  'removeEnv',
  'setSetting',
  'removeSetting',
  'setAgent',
  'removeAgent',
]);
const MAX_BODY = 64 * 1024;
const ASSETS = new Map<string, { type: string; data: string }>([
  ['/', { type: 'text/html; charset=utf-8', data: indexHtml }],
  ['/app.js', { type: 'text/javascript; charset=utf-8', data: appJs }],
  ['/style.css', { type: 'text/css; charset=utf-8', data: styleCss }],
]);
const token = (): string => randomBytes(32).toString('base64url');
function equal(actual: unknown, expected: string | undefined): boolean {
  if (typeof actual !== 'string' || typeof expected !== 'string') return false;
  const left = Buffer.from(actual);
  const right = Buffer.from(expected);
  return left.length === right.length && timingSafeEqual(left, right);
}
function failure(status: number, code: string): HttpFailure {
  return Object.assign(new Error(code), { status, code });
}
async function body(
  request: IncomingMessage
): Promise<Record<string, unknown>> {
  if (request.headers['content-type'] !== 'application/json')
    throw failure(415, 'JSON_REQUIRED');
  if (Number(request.headers['content-length']) > MAX_BODY)
    throw failure(413, 'BODY_TOO_LARGE');
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request as AsyncIterable<Buffer>) {
    size += chunk.length;
    if (size > MAX_BODY) throw failure(413, 'BODY_TOO_LARGE');
    chunks.push(chunk);
  }
  try {
    const value: unknown = JSON.parse(Buffer.concat(chunks).toString('utf8'));
    if (!value || typeof value !== 'object' || Array.isArray(value))
      throw new Error();
    return value as Record<string, unknown>;
  } catch {
    throw failure(400, 'INVALID_JSON');
  }
}
function safePayload(value: unknown, depth = 0): boolean {
  if (depth > 12) return false;
  if (value && typeof value === 'object') {
    return Object.entries(value).every(
      ([key, entry]) =>
        !['__proto__', 'constructor', 'prototype'].includes(key) &&
        safePayload(entry, depth + 1)
    );
  }
  return typeof value !== 'string' || value.length <= 16384;
}
const MESSAGES: Record<string, string> = {
  CONFLICT:
    'The file changed. Refresh and review the current configuration before saving.',
  INVALID_INPUT: 'The configuration value is invalid.',
  FORBIDDEN: 'This change is not allowed for this scope.',
  CLOSED: 'The configuration session has ended. Run the command again.',
};
const ERROR_CODES = new Set([
  'CONFLICT',
  'INVALID_INPUT',
  'FORBIDDEN',
  'CLOSED',
  'JSON_REQUIRED',
  'BODY_TOO_LARGE',
  'NOT_FOUND',
  'METHOD_NOT_ALLOWED',
  'UNAUTHORIZED',
  'BUSY',
  'INVALID_JSON',
  'REQUEST_FAILED',
  'TIMEOUT',
]);

/** Starts a single temporary session. Configuration interpretation stays in the injected native backend. */
export async function startConfigView({
  request: backend,
  open,
  idleTimeoutMs = 15 * 60 * 1000,
  signal,
  onReady,
}: ConfigViewOptions): Promise<ConfigView> {
  if (typeof backend !== 'function')
    throw new TypeError('A native request function is required');
  if (!Number.isFinite(idleTimeoutMs) || idleTimeoutMs < 1)
    throw new TypeError('Invalid session timeout');
  if (signal?.aborted) throw new Error('Configuration session cancelled');
  let bootstrap: string | undefined = token();
  let session: string | undefined;
  let origin = '';
  let host = '';
  let active = 0;
  const executing = new Set<AbortController>();
  let closing = false;
  let timer: NodeJS.Timeout | undefined;
  let resolveClosed!: () => void;
  const closed = new Promise<void>(resolve => {
    resolveClosed = resolve;
  });
  const headers = {
    'Content-Security-Policy':
      "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
    'Cache-Control': 'no-store',
    'X-Content-Type-Options': 'nosniff',
    'Referrer-Policy': 'no-referrer',
    'Cross-Origin-Resource-Policy': 'same-origin',
    'X-Frame-Options': 'DENY',
    'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
  };
  function send(
    response: ServerResponse,
    status: number,
    value: unknown,
    type = 'application/json; charset=utf-8'
  ): void {
    if (response.destroyed || response.writableEnded) return;
    response.writeHead(status, { ...headers, 'Content-Type': type });
    response.end(
      type.startsWith('application/json')
        ? JSON.stringify(value)
        : (value as string)
    );
  }
  const server = createServer(async (incoming, response) => {
    let admitted = false;
    try {
      if (closing) throw failure(410, 'CLOSED');
      if (
        incoming.headers.host !== host ||
        incoming.rawHeaders.filter(
          (entry, index) => index % 2 === 0 && entry.toLowerCase() === 'host'
        ).length !== 1
      )
        throw failure(403, 'FORBIDDEN');
      const requestOrigin = incoming.headers.origin;
      if (requestOrigin !== undefined && requestOrigin !== origin)
        throw failure(403, 'FORBIDDEN');
      const fetchSite = incoming.headers['sec-fetch-site'];
      if (
        typeof fetchSite === 'string' &&
        fetchSite &&
        !['same-origin', 'none'].includes(fetchSite)
      )
        throw failure(403, 'FORBIDDEN');
      const path = incoming.url ?? '';
      const asset = incoming.method === 'GET' ? ASSETS.get(path) : undefined;
      if (asset) {
        send(response, 200, asset.data, asset.type);
        return;
      }
      if (!['/api/session', '/api/request', '/api/close'].includes(path))
        throw failure(404, 'NOT_FOUND');
      if (incoming.method !== 'POST') throw failure(405, 'METHOD_NOT_ALLOWED');
      if (requestOrigin !== origin) throw failure(403, 'FORBIDDEN');
      const credential = incoming.headers['x-octocode-session'];
      if (!equal(credential, path === '/api/session' ? bootstrap : session))
        throw failure(401, 'UNAUTHORIZED');
      if (active >= 4) throw failure(429, 'BUSY');
      active++;
      admitted = true;
      const payload = await body(incoming);
      if (!safePayload(payload)) throw failure(400, 'INVALID_INPUT');
      if (path === '/api/session') {
        // Recheck after body streaming so concurrent attempts cannot replay bootstrap.
        if (!equal(credential, bootstrap)) throw failure(401, 'UNAUTHORIZED');
        bootstrap = undefined;
        session = token();
        touch();
        send(response, 200, { token: session });
      } else if (path === '/api/close') {
        send(response, 200, { closed: true });
        void close();
      } else {
        if (
          typeof payload.operation !== 'string' ||
          !OPERATIONS.has(payload.operation)
        )
          throw failure(400, 'INVALID_INPUT');
        touch();
        const cancellation = new AbortController();
        const disconnect = (): void => {
          if (!response.writableEnded) cancellation.abort();
        };
        response.once('close', disconnect);
        executing.add(cancellation);
        let deadline: NodeJS.Timeout | undefined;
        const work = Promise.resolve().then(() =>
          backend(payload, { signal: cancellation.signal })
        );
        // Retain admission until the backend settles, even if a deadline or close
        // has ended the browser request. An uncooperative backend cannot fan out.
        admitted = false;
        void work
          .finally(() => {
            active--;
            executing.delete(cancellation);
            response.removeListener('close', disconnect);
          })
          .catch(() => {});
        const timeout = new Promise<never>((_, reject) => {
          deadline = setTimeout(() => {
            cancellation.abort();
            reject(failure(408, 'TIMEOUT'));
          }, 30000);
          deadline.unref();
        });
        let result: unknown;
        try {
          result = await Promise.race([work, timeout]);
        } finally {
          clearTimeout(deadline);
        }
        if (closing) throw failure(410, 'CLOSED');
        send(response, 200, result ?? { ok: true });
        touch();
      }
    } catch (caught) {
      // Native errors may contain user values. Only machine codes cross this boundary.
      const error = (caught ?? {}) as Partial<HttpFailure> & { safe?: unknown };
      const code =
        typeof error.code === 'string' && ERROR_CODES.has(error.code)
          ? error.code
          : 'REQUEST_FAILED';
      const safeMessage =
        error.safe === true && typeof error.message === 'string'
          ? error.message
          : undefined;
      const status =
        typeof error.status === 'number' &&
        Number.isInteger(error.status) &&
        error.status >= 400 &&
        error.status <= 599
          ? error.status
          : code === 'CONFLICT'
            ? 409
            : 400;
      send(response, status, {
        error: {
          code,
          message:
            safeMessage ??
            MESSAGES[code] ??
            'The request could not be completed.',
        },
      });
    } finally {
      if (admitted) active--;
    }
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 10000;
  server.timeout = 15000;
  server.maxHeadersCount = 30;
  server.on('timeout', socket => socket.destroy());
  function touch(): void {
    clearTimeout(timer);
    timer = setTimeout(() => void close(), idleTimeoutMs);
    timer.unref();
  }
  async function close(): Promise<void> {
    if (closing) return closed;
    closing = true;
    bootstrap = session = undefined;
    clearTimeout(timer);
    for (const cancellation of executing) cancellation.abort();
    signal?.removeEventListener('abort', abort);
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    resolveClosed();
  }
  function abort(): void {
    void close();
  }
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen({ host: '127.0.0.1', port: 0, exclusive: true }, () => {
      server.removeListener('error', reject);
      resolve();
    });
  });
  host = `127.0.0.1:${(server.address() as AddressInfo).port}`;
  origin = `http://${host}`;
  const view: ConfigView = {
    origin,
    url: `${origin}/#${bootstrap}`,
    close,
    closed,
  };
  signal?.addEventListener('abort', abort, { once: true });
  if (signal?.aborted) await close();
  touch();
  try {
    await onReady?.(view);
    if (!closing) await open?.(view.url);
  } catch (error) {
    await close();
    throw error;
  }
  return view;
}
