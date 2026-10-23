/** Loopback-only, token-scoped read-only dashboard. */
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createServer, type Server, type Socket } from 'node:net';
import appJs from '../../assets/dashboard/app.js' with { type: 'text' };
import indexHtml from '../../assets/dashboard/index.html';
import styleCss from '../../assets/dashboard/style.css';
import { databasePath } from './config.js';
import type { DeliveryRuntime } from './delivery-types.js';
import { stopEvent } from './loop-control.js';
import { ValidationError } from './errors.js';
import type { StoreHost } from './store-types.js';
import { compact, decodeUtf8 } from './json-text.js';
import * as viewData from './view-data.js';
import type { ViewInput } from './catalog-types.generated.js';

const HEADER_LIMIT = 8192;
const HEADER_DEADLINE_MS = 2000;
const SECURITY_HEADERS = "Connection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n\r\n";
const ASSETS: Record<string, readonly [string, string]> = {
  '': [indexHtml, 'text/html; charset=utf-8'],
  'app.js': [appJs, 'text/javascript; charset=utf-8'],
  'style.css': [styleCss, 'text/css; charset=utf-8'],
};

interface ViewArgs {
  database?: string;
  workspace: string;
}

interface ViewRuntime extends Pick<DeliveryRuntime, 'output' | 'open'> {
  /** Starts the system browser on `url`; resolves when the launcher exits and rejects when it fails. */
  openBrowser(url: string): Promise<void>;
}

/** The system browser launcher for this platform. */
export function launchBrowser(url: string): Promise<void> {
  const [command, ...args] = process.platform === 'darwin' ? ['open', url] : process.platform === 'win32' ? ['cmd', '/C', 'start', '', url] : ['xdg-open', url];
  return new Promise((resolve, reject) => {
    const child = spawn(command!, args, { stdio: 'ignore' });
    child.once('error', reject);
    child.once('close', code => (code ? reject(new Error('Browser launcher failed')) : resolve()));
  });
}

function respond(stream: Socket, status: string, mime: string, body: string | Buffer): void {
  const bytes = typeof body === 'string' ? Buffer.from(body) : body;
  stream.end(Buffer.concat([Buffer.from(`HTTP/1.1 ${status}\r\nContent-Type: ${mime}\r\nContent-Length: ${bytes.length}\r\n${SECURITY_HEADERS}`), bytes]));
}

/** Form decoding: `+` is a space, percent escapes are strict, and the bytes must be UTF-8. */
function decodeQuery(value: string): string {
  if (/%(?![0-9A-Fa-f]{2})/.test(value)) throw new ValidationError('Invalid percent encoding');
  const bytes: number[] = [];
  for (const [, hex, character] of value.replaceAll('+', ' ').matchAll(/%([0-9A-Fa-f]{2})|([^%])/gsu)) bytes.push(...(hex ? [Number.parseInt(hex, 16)] : Buffer.from(character!)));
  return decodeUtf8(Uint8Array.from(bytes));
}

function api(store: StoreHost, route: string): viewData.ViewSummary | viewData.ViewPage {
  if (route === 'api/summary') return viewData.summary(store);
  const separator = route.indexOf('?');
  const path = separator < 0 ? route : route.slice(0, separator);
  const search = separator < 0 ? '' : route.slice(separator + 1);
  if (!path.startsWith('api/')) throw new ValidationError('Unknown route');
  let after: string | null = null;
  let agent: string | null = null;
  const filters: viewData.ViewFilters = {};
  for (const part of search.split('&').filter(Boolean)) {
    const equals = part.indexOf('=');
    if (equals < 0) throw new ValidationError('Invalid query');
    const key = part.slice(0, equals);
    const value = part.slice(equals + 1);
    if (key === 'after' && after === null) after = value;
    else if (key === 'agent' && agent === null) agent = value;
    else if ((key === 'q' || key === 'conversation' || key === 'status') && !(key in filters)) filters[key] = decodeQuery(value);
    else throw new ValidationError('Unknown or duplicate filter');
  }
  return viewData.page(store, path.slice(4), after, agent, filters);
}

function handle(stream: Socket, head: string, store: StoreHost, host: string, prefix: string): void {
  const lines = head.split('\r\n');
  const first = lines[0]!.trim().split(/\s+/);
  if (first.length !== 3 || first[0] !== 'GET') return respond(stream, '405 Method Not Allowed', 'text/plain', 'Read-only view: GET only');
  const headers: [string, string][] = [];
  for (const line of lines.slice(1)) {
    if (!line) break;
    const colon = line.indexOf(':');
    if (colon >= 0) headers.push([line.slice(0, colon).toLowerCase(), line.slice(colon + 1).trim()]);
  }
  const hosts = headers.filter(([key]) => key === 'host').map(([, value]) => value);
  if (hosts.length !== 1 || hosts[0] !== host || headers.some(([key, value]) => key === 'origin' && value !== 'http://' + host)) return respond(stream, '403 Forbidden', 'text/plain', 'Local origin required');
  const target = first[1]!;
  if (!target.startsWith(prefix)) return respond(stream, '404 Not Found', 'text/plain', 'Not found');
  const route = target.slice(prefix.length);
  const asset = Object.hasOwn(ASSETS, route) ? ASSETS[route] : undefined;
  if (asset) return respond(stream, '200 OK', asset[1], asset[0]);
  let status = '200 OK';
  let value: viewData.ViewSummary | viewData.ViewPage | { error: string };
  try {
    value = api(store, route);
  } catch (error) {
    status = '400 Bad Request';
    value = { error: (error as Error).message };
  }
  respond(stream, status, 'application/json', compact(value));
}

/** Reads one request head within the bounded size and time, then answers it. */
function serveConnection(stream: Socket, store: StoreHost, host: string, prefix: string): void {
  const chunks: Buffer[] = [];
  let size = 0;
  const end = (reason: string): void => {
    process.stderr.write(`View request ended: ${reason}\n`);
    stream.destroy();
  };
  const timer = setTimeout(() => end('Request header limit'), HEADER_DEADLINE_MS);
  stream.on('error', () => clearTimeout(timer));
  stream.on('close', () => clearTimeout(timer));
  const onData = (chunk: Buffer): void => {
    chunks.push(chunk);
    size += chunk.length;
    const data = Buffer.concat(chunks);
    const terminator = data.indexOf('\r\n\r\n');
    if (terminator < 0 && size < HEADER_LIMIT) return;
    stream.off('data', onData);
    // The head, terminator included, must fit the limit however the bytes were chunked.
    if (terminator < 0 || terminator + 4 > HEADER_LIMIT) return end('Request header limit');
    clearTimeout(timer);
    try {
      handle(stream, decodeUtf8(data), store, host, prefix);
    } catch (error) {
      end((error as Error).message);
    }
  };
  stream.on('data', onData);
}

export async function run(args: ViewArgs, input: ViewInput, runtime: ViewRuntime): Promise<void> {
  const store = runtime.open(databasePath(args.database), args.workspace, { readOnly: true });
  const stop = stopEvent();
  const sockets = new Set<Socket>();
  const server: Server = createServer(stream => {
    sockets.add(stream);
    stream.once('close', () => sockets.delete(stream));
    serveConnection(stream, store, host, prefix);
  });
  let host = '';
  let prefix = '';
  try {
    await new Promise<void>((resolve, reject) => {
      server.once('error', reject);
      server.listen(input.port ?? 0, '127.0.0.1', resolve);
    });
    host = '127.0.0.1:' + (server.address() as { port: number }).port;
    prefix = '/' + randomUUID() + '/';
    const url = 'http://' + host + prefix;
    runtime.output({ url, pid: process.pid, workspace: store.workspace, database: store.database, readOnly: true, stop: 'Ctrl+C' });
    if (input.open ?? true) runtime.openBrowser(url).catch(() => process.stderr.write('Browser could not open; use the printed local URL\n'));
    if (!stop.signal.aborted) await new Promise<void>(resolve => stop.signal.addEventListener('abort', () => resolve(), { once: true }));
  } finally {
    stop.dispose();
    for (const socket of sockets) socket.destroy();
    server.close();
    store.db.close();
  }
}
