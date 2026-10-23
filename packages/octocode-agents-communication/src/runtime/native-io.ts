/** Local native endpoints: loopback and owned-socket validation, and bounded socket channels with monotonic deadlines. */
import { lstatSync } from 'node:fs';
import { connect as connectSocket, isIP, type Socket } from 'node:net';
import { isAbsolute } from 'node:path';
import { ValidationError } from './errors.js';
import { monotonic } from './timing.js';
import { isObject, type JsonObject } from './validation.js';

export const UNIX_PREFIX = 'unix://';
export const SOCKET_IO_MS = 5000;
export const TCP_CONNECT_MS = 3000;
const HIGH_WATER = 256 * 1024;

function loopbackAddress(host: string): string | null {
  if (isIP(host) === 4) return host.startsWith('127.') ? host : null;
  if (isIP(host) !== 6) return null;
  const address = new URL(`http://[${host}]`).hostname.slice(1, -1);
  const mapped = /^::ffff:7f[0-9a-f]{2}:[0-9a-f]{1,4}$/.exec(address);
  return address === '::1' || mapped ? address : null;
}

/** The IP and port of a loopback endpoint `<scheme>://<loopback-IP>:<port>/`; credentials, path, query and fragment are refused. */
export function loopback(endpoint: string, scheme: string, allowLocalhost: boolean): { host: string; port: number } {
  const parts = /^([A-Za-z][A-Za-z0-9+.-]*):\/\/([^/?#]*)(\/?)$/.exec(endpoint);
  let result: { host: string; port: number } | null = null;
  if (parts && parts[1].toLowerCase() === scheme) {
    const netloc = parts[2];
    const found = /^\[([^\]]*)\]:([0-9]+)$/.exec(netloc) ?? /^([^:@[\]]*):([0-9]+)$/.exec(netloc);
    const port = found ? Number(found[2]) : 0;
    if (found && port > 0 && port <= 65535) {
      const host = found[1].toLowerCase();
      const address = loopbackAddress(allowLocalhost && host === 'localhost' ? '127.0.0.1' : host);
      if (address) result = { host: address, port };
    }
  }
  if (!result) throw new ValidationError(`endpoint must be ${scheme}://<loopback-IP${allowLocalhost ? '|localhost' : ''}>:<port>/; no credentials, path, query or fragment`);
  return result;
}

function ownedSocket(path: string): void {
  if (typeof process.geteuid !== 'function') throw new ValidationError('Native Unix socket delivery is unavailable on this platform; use host hooks');
  const info = lstatSync(path);
  if (!info.isSocket() || info.uid !== process.geteuid()) throw new ValidationError('Endpoint must be a socket owned by this OS user, not a symlink');
}

export function socketPath(endpoint: string): string {
  return endpoint.startsWith(UNIX_PREFIX) ? endpoint.slice(UNIX_PREFIX.length) : endpoint;
}

export function validate(mode: string, endpoint?: string | null): void {
  if (mode === 'raw') {
    if (endpoint !== undefined && endpoint !== null) throw new ValidationError('Raw hooks do not have an endpoint');
    return;
  }
  if (!endpoint) throw new ValidationError('Native attachment requires endpoint');
  if (mode === 'opencode') loopback(endpoint, 'http', false);
  else if (mode === 'claude' || mode === 'grok' || (mode === 'codex' && endpoint.startsWith(UNIX_PREFIX))) {
    const path = socketPath(endpoint);
    if (!isAbsolute(path) || Buffer.byteLength(path) > 103) throw new ValidationError('Unix socket endpoint must be an absolute path of at most 103 bytes');
    if (mode === 'grok') ownedSocket(path);
  } else if (mode === 'codex') loopback(endpoint, 'ws', true);
  else throw new ValidationError('Unknown native transport');
}

export function validateSession(session: string): void {
  if (!/^ses_[A-Za-z0-9_]{1,252}$/.test(session)) throw new ValidationError('OpenCode vendorSession must be its existing ses_ identifier');
}

export class EndOfStream extends Error {
  override name = 'EndOfStream';
}

/**
 * A socket read through a bounded buffer with absolute monotonic deadlines (a per-call timeout). Reading is
 * demand-driven: the socket pauses once the buffer passes HIGH_WATER and resumes only when a consumer waits for
 * more bytes, so a peer cannot grow the buffer unasked while a frame the consumer is still assembling always completes.
 * Callers bound what they assemble (frame limits), which bounds the buffer.
 */
export class Channel {
  private buffer: Buffer = Buffer.alloc(0);
  private failure: Error | null = null;
  private waiter: (() => void) | null = null;

  constructor(readonly socket: Socket, closedMessage: string) {
    socket.on('data', chunk => {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      if (this.buffer.length > HIGH_WATER) socket.pause();
      this.wake();
    });
    const closed = (): void => this.fail(new EndOfStream(closedMessage));
    socket.on('end', closed);
    socket.on('close', closed);
    socket.on('error', error => this.fail(error));
  }

  get length(): number {
    return this.buffer.length;
  }

  peek(count: number): Buffer {
    return this.buffer.subarray(0, count);
  }

  take(count: number): Buffer {
    const value = Buffer.from(this.buffer.subarray(0, count));
    this.buffer = this.buffer.subarray(count);
    return value;
  }

  private fail(error: Error): void {
    this.failure ??= error;
    this.wake();
  }

  private wake(): void {
    const waiter = this.waiter;
    this.waiter = null;
    waiter?.();
  }

  /** Waits for more bytes: true on progress, false at the deadline; throws once the peer closed with nothing more to read. */
  async fill(deadline: number): Promise<boolean> {
    if (this.failure) throw this.failure;
    const remaining = deadline - monotonic();
    if (remaining <= 0) return false;
    const before = this.buffer.length;
    this.socket.resume();
    await new Promise<void>(resolve => {
      const timer = setTimeout(done, remaining);
      this.waiter = done;
      function done(): void {
        clearTimeout(timer);
        resolve();
      }
    });
    this.waiter = null;
    if (this.buffer.length > before) return true;
    if (this.failure) throw this.failure;
    return false;
  }

  write(data: Uint8Array, deadline: number, timeoutMessage: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const remaining = deadline - monotonic();
      if (remaining <= 0) return reject(new Error(timeoutMessage));
      const timer = setTimeout(() => reject(new Error(timeoutMessage)), remaining);
      this.socket.write(data, error => {
        clearTimeout(timer);
        if (error) reject(error);
        else resolve();
      });
    });
  }

  close(): void {
    this.socket.destroy();
  }
}

export function connect(options: { path: string } | { host: string; port: number }, timeoutMs: number, closedMessage: string): Promise<Channel> {
  return new Promise((resolve, reject) => {
    const socket = connectSocket(options);
    const channel = new Channel(socket, closedMessage);
    const timer = setTimeout(() => {
      socket.destroy();
      reject(new Error('Native I/O timed out'));
    }, timeoutMs);
    socket.once('connect', () => {
      clearTimeout(timer);
      resolve(channel);
    });
    socket.once('error', error => {
      clearTimeout(timer);
      reject(error);
    });
  });
}

export function unix(path: string, closedMessage: string): Promise<Channel> {
  ownedSocket(path);
  return connect({ path }, SOCKET_IO_MS, closedMessage);
}

export function object(value: unknown, what: string): JsonObject {
  if (!isObject(value)) throw new TypeError(what + ' is not a JSON object');
  return value;
}

export const isOneOf = (value: unknown, allowed: readonly string[]): boolean => typeof value === 'string' && allowed.includes(value);
