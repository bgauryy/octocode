// Loopback and Unix-socket test servers for the native transport tests, and their shared assertions.
import assert from 'node:assert/strict';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { createServer } from 'node:net';
import type { AddressInfo, Server, Socket } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { TestContext } from 'node:test';

/** A fresh real-path workspace removed after the test. */
export function workspace(t: TestContext) {
  const path = realpathSync(mkdtempSync(join(tmpdir(), 'native-transports-')));
  t.after(() => rmSync(path, { recursive: true, force: true }));
  return path;
}

/** Where a test server listens: a loopback port, or a Unix socket path. */
export type Target = readonly [port: number, host: string] | readonly [path: string];

export const listening = <Listener extends Server>(server: Listener, ...target: Target) => new Promise<Listener>(resolve => {
  const ready = () => resolve(server);
  if (target.length === 2) server.listen(target[0], target[1], ready); else server.listen(target[0], ready);
});
export const portOf = (server: { address(): AddressInfo | string | null }): number => {
  const address = server.address();
  assert.ok(address !== null && typeof address === 'object', 'the server listens on a port');
  return address.port;
};
export function serve(t: TestContext, handler: (socket: Socket) => void, ...target: Target) {
  const sockets = new Set<Socket>();
  const server = createServer(socket => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => undefined);
    handler(socket);
  });
  t.after(() => { for (const socket of sockets) socket.destroy(); server.close(); });
  return listening(server, ...target);
}

/** Asserts that `promise` rejects with an Error whose message matches `pattern` (and whose name is `name`, when given). */
export function rejects(promise: Promise<unknown>, pattern: RegExp, name?: string) {
  return assert.rejects(promise, (error: unknown) => {
    assert.ok(error instanceof Error);
    if (name) assert.equal(error.name, name);
    assert.match(error.message, pattern);
    return true;
  });
}
