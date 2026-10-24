// Test support for the runtime store: temporary stores over real databases.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, realpathSync } from 'node:fs';
import { join } from 'node:path';
import type { TestContext } from 'node:test';
import { root, tempWorkspace, withReasoning } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

export const bundlePath = join(root, 'dist/runtime.js');
export const runtime = await import('../dist/runtime.js');
export const { Store } = runtime.store;
export const { ValidationError } = runtime;
type StoreInstance = InstanceType<typeof Store>;

/** Runs `name` on `store` as `session` after the catalog validates the input, as the CLI and MCP entries do. */
// Negative tests pass malformed input on purpose: validation, not the type, refuses it.
export async function callStore(store: StoreInstance, session: string, name: string, input: Record<string, unknown> = {}): Promise<JsonRecord> {
  const checked = runtime.catalog.command(runtime.catalog.commandName(name), withReasoning(name, input));
  return store.call(session, checked.name, checked.input);
}

/** A real workspace (a Git repository when `git`) with a fresh database and a store bound to it. */
export function fixture(t: TestContext, { git = false }: { git?: boolean } = {}) {
  const workspace = realpathSync(tempWorkspace(t, 'store-', { real: true }));
  if (git) execFileSync('git', ['init', '-q', '-b', 'main', workspace]);
  const file = join(workspace, 'store.sqlite');
  const stores: StoreInstance[] = [];
  const open = (path = workspace, create = true) => {
    const store = new Store(file, path, false, create);
    stores.push(store);
    return store;
  };
  t.after(() => { for (const store of stores) try { store.close(); } catch { /* already closed */ } });
  const store = open();
  const call = (session: string, name: string, input: Record<string, unknown> = {}) => callStore(store, session, name, input);
  const join_ = async (name: string) => (await call('', 'join', { name, vendor: 'generic' })).id;
  return { workspace, file, store, open, call, join: join_, mkdir: (path: string) => mkdirSync(join(workspace, path), { recursive: true }) };
}

/** Asserts that `promise` rejects with a ValidationError whose message matches `pattern`. */
export const refuses = (promise: Promise<unknown>, pattern: RegExp, message?: string) => assert.rejects(promise, (error: unknown) => {
  assert.ok(error instanceof ValidationError, `${message ?? ''} expected a ValidationError, got ${error instanceof Error ? error.stack : error}`);
  assert.match(error.message, pattern, message);
  return true;
});

/** A store fixture with three joined peers (alice, bob, carol) and inbox readers keyed by session. */
export async function trio(t: TestContext, options?: Parameters<typeof fixture>[1]) {
  const f = fixture(t, options);
  const [alice, bob, carol] = [await f.join('alice'), await f.join('bob'), await f.join('carol')];
  const inbox = async (session: string): Promise<JsonRecord[]> => (await f.call(session, 'inbox')).items;
  const find = async (session: string, id: number): Promise<JsonRecord> => {
    const item = (await inbox(session)).find(candidate => candidate.id === id);
    assert.ok(item, `Inbox of ${session} has no message ${id}`);
    return item;
  };
  const body = async (session: string, id: number) => (await find(session, id)).body;
  return { ...f, alice, bob, carol, inbox, find, body };
}
