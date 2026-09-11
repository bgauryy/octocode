import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import { createPiHookRuntime, createPiHookReviewStore, type PiHookReviewStore } from '../src/adapters/pi-hook-runtime.js';

const roots: string[] = [];
let restoreProcesses: (() => void) | undefined;
afterEach(() => { restoreProcesses?.(); vi.unstubAllEnvs(); for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });

function fixture(command: string, event = 'tool_call', timeout = 2) {
  restoreProcesses = allowLocalFixtureProcesses();
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'pi-command-hook-')));
  roots.push(root);
  const workspace = path.join(root, 'workspace');
  const octocodeHome = path.join(root, 'octocode');
  const file = path.join(workspace, '.agents', 'hooks', 'guard.json');
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const write = (nextCommand: string) => fs.writeFileSync(file, JSON.stringify({ hooks: { [event]: [{ type: 'command', command: nextCommand, timeout }] } }));
  write(command);
  const reviews = new Map<string, { revision: string; enabled: boolean }>();
  const state: PiHookReviewStore = {
    get: id => reviews.get(id),
    review: source => { reviews.set(source.sourceId, { revision: source.revision, enabled: true }); },
    setEnabled: (id, enabled) => { const current = reviews.get(id); if (!current) throw new Error('Review required'); reviews.set(id, { ...current, enabled }); },
  };
  const runtime = createPiHookRuntime({ workspace, octocodeHome, userCodexDir: path.join(root, 'none'), state });
  runtime.refresh({ trusted: true });
  const source = runtime.snapshot().sources[0]!;
  const review = () => runtime.review(source.id, runtime.snapshot().sources[0]!.revision);
  return { root, workspace, runtime, review, write, source, state };
}

describe('declarative Pi hooks', { concurrent: false }, () => {
  it('executes reviewed shell commands once, gives hooks event JSON, and reloads without duplicate registration', async () => {
    const f = fixture('read payload; printf "%s\\n" "$payload" >> calls.txt', 'tool_execution_end');
    f.review();
    const listeners = new Map<string, (...args: unknown[]) => unknown>();
    const composer = { on: (event: string, _name: string, handler: (...args: unknown[]) => unknown) => { expect(listeners.has(event)).toBe(false); listeners.set(event, handler); } };
    f.runtime.bind(composer);
    f.runtime.bind(composer);
    f.runtime.refresh({ trusted: true });
    await listeners.get('tool_execution_end')!({ toolName: 'file', result: 'done' }, { cwd: f.workspace, isProjectTrusted: () => true });
    const calls = fs.readFileSync(path.join(f.workspace, 'calls.txt'), 'utf8').trim().split('\n');
    expect(calls).toHaveLength(1);
    expect(JSON.parse(calls[0]!)).toMatchObject({ hook_event_name: 'PostToolUse', tool_name: 'file' });
    f.runtime.dispose();
  });

  it('blocks on exit 2 and JSON deny, while pending, disabled, stale, and untrusted hooks do not execute', async () => {
    const f = fixture('printf \'{"decision":"block","reason":"fixture denial"}\'');
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => true })).toBeUndefined();
    f.review();
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => true })).toEqual({ block: true, reason: 'fixture denial' });
    f.runtime.setEnabled(f.source.id, false);
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => true })).toBeUndefined();
    f.runtime.setEnabled(f.source.id, true);
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => false })).toBeUndefined();
    f.write('exit 2');
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => true })).toBeUndefined();
    expect(f.runtime.snapshot().sources[0]?.status).toBe('pending-review');
    f.review();
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => true })).toMatchObject({ block: true });
    f.runtime.dispose();
  });

  it('times out and cancels real child processes, with fail-closed pre-tool decisions', async () => {
    const f = fixture('sleep 5', 'tool_call', 0.03);
    f.review();
    expect(await f.runtime.dispatch('tool_call', {}, { isProjectTrusted: () => true })).toMatchObject({ block: true });
    expect(f.runtime.snapshot().receipts.at(-1)?.outcome).toBe('timeout');
    const abort = new AbortController();
    const pending = f.runtime.dispatch('tool_call', {}, { signal: abort.signal, isProjectTrusted: () => true });
    abort.abort();
    expect(await pending).toMatchObject({ block: true });
    expect(f.runtime.snapshot().receipts.at(-1)?.outcome).toBe('cancelled');
    f.runtime.dispose();
  });

  it('retains reviewed enablement in its provided store across runtime reconstruction', () => {
    const f = fixture('exit 0');
    f.review();
    f.runtime.setEnabled(f.source.id, false);
    f.runtime.dispose();
    const next = createPiHookRuntime({ workspace: f.workspace, octocodeHome: path.join(f.root, 'octocode'), userCodexDir: path.join(f.root, 'none'), state: f.state });
    next.refresh({ trusted: true });
    expect(next.snapshot().sources[0]?.status).toBe('disabled');
    next.dispose();
  });

  it('persists reviewed revisions and enablement in the real extension state database', () => {
    const f = fixture('exit 0');
    vi.stubEnv('OCTOCODE_HOME', path.join(f.root, 'octocode'));
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
    const first = createPiHookReviewStore(f.workspace);
    first.review({ kind: 'hook', sourceId: f.source.id, revision: f.source.revision, name: f.source.name, path: f.source.path });
    first.setEnabled(f.source.id, false);
    expect(createPiHookReviewStore(f.workspace).get(f.source.id)).toMatchObject({ revision: f.source.revision, enabled: false });
    expect(fs.existsSync(path.join(f.root, 'octocode', 'extension', 'state', 'extension.sqlite3'))).toBe(true);
    f.runtime.dispose();
  });

  it('reports unreadable persisted reviews without executing hooks or changing the broken database', async () => {
    const f = fixture('touch MUST_NOT_EXECUTE');
    vi.stubEnv('OCTOCODE_HOME', path.join(f.root, 'octocode'));
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
    const dbPath = path.join(f.root, 'octocode', 'extension', 'state', 'extension.sqlite3');
    fs.mkdirSync(path.dirname(dbPath), { recursive: true });
    fs.writeFileSync(dbPath, 'unreadable-fixture-database');
    const runtime = createPiHookRuntime({ workspace: f.workspace, octocodeHome: path.join(f.root, 'octocode'), userCodexDir: path.join(f.root, 'none') });
    const snapshot = runtime.refresh({ trusted: true });
    expect(snapshot.sources[0]).toMatchObject({ status: 'unavailable', enabled: false });
    expect(snapshot.sources[0]?.diagnostics).toContain('Hook review state is unavailable; commands remain disabled');
    expect(await runtime.dispatch('tool_call', {}, { cwd: f.workspace, isProjectTrusted: () => true })).toBeUndefined();
    expect(fs.existsSync(path.join(f.workspace, 'MUST_NOT_EXECUTE'))).toBe(false);
    expect(() => runtime.review(f.source.id, f.source.revision)).toThrow();
    expect(() => runtime.setEnabled(f.source.id, true)).toThrow();
    expect(fs.readFileSync(dbPath, 'utf8')).toBe('unreadable-fixture-database');
    runtime.dispose();
    f.runtime.dispose();
  });

  it('reports malformed matchers before dispatch without throwing a tool-call error', async () => {
    const f = fixture('touch MUST_NOT_EXECUTE');
    fs.writeFileSync(path.join(f.workspace, '.agents', 'hooks', 'guard.json'), JSON.stringify({ hooks: { tool_call: [{ matcher: '[', hooks: [{ type: 'command', command: 'touch MUST_NOT_EXECUTE' }] }] } }));
    const snapshot = f.runtime.refresh({ trusted: true });
    expect(snapshot.sources).toHaveLength(0);
    expect(snapshot.errors).toEqual([expect.objectContaining({ message: 'Invalid matcher for PreToolUse' })]);
    expect(await f.runtime.dispatch('tool_call', { toolName: 'file' }, { cwd: f.workspace, isProjectTrusted: () => true })).toBeUndefined();
    expect(fs.existsSync(path.join(f.workspace, 'MUST_NOT_EXECUTE'))).toBe(false);
    f.runtime.dispose();
  });
});
