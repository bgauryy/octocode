import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { executeAwarenessCommand, type AwarenessCommandResult } from '../src/command-api.js';
import { executeAwarenessCli } from '../src/command-cli.js';

let root: string;
let context: { database: string; workspace: string; agentId: string; compact: boolean };
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'awareness-adapters-'));
  context = { database: join(root, 'awareness.sqlite3'), workspace: root, agentId: 'owner', compact: true };
  vi.stubEnv('OCTOCODE_HOME', join(root, 'home'));
  vi.stubEnv('OCTOCODE_AGENT_ID', undefined);
});
afterEach(() => { vi.unstubAllEnvs(); rmSync(root, { recursive: true, force: true }); });
const call = (command: string, params?: Record<string, unknown>) => executeAwarenessCommand({ command, params }, context);
function ok(result: AwarenessCommandResult): Record<string, any> {
  expect(result.exitCode, JSON.stringify(result)).toBe(0);
  return result.payload as Record<string, any>;
}

describe('public command adapters against real isolated stores', () => {
  it('preserves repeated files, scalar flags and explicit booleans from shell to memory recall', async () => {
    const bindings = ['--db', context.database, '--workspace', root];
    const recorded = ok(await executeAwarenessCli(['memory', 'record', ...bindings, '--agent-id', 'owner',
      '--task-context', 'parser', '--observation', 'Parser needs both fixture files', '--importance', '7',
      '--file', 'a.ts', '--file', 'b.ts', '--capture-fingerprint', 'false', '--compact', 'yes']));
    const recalled = ok(await executeAwarenessCli(['memory', 'recall', ...bindings, '--query', 'parser',
      '--file', 'a.ts', '--limit', '1', '--full', 'true', '--check-fingerprint', 'false', '--compact', '1']));
    expect(recalled.memories).toHaveLength(1);
    expect(JSON.stringify(recorded)).toContain(recalled.memories[0].memory_id);
    expect(ok(await call('memory recall', { query: 'parser', file: ['b.ts'], full: true })).count).toBe(1);
  });

  it('rejects malformed shell input before creating a store and exposes canonical corrections', async () => {
    for (const args of [
      ['not-a-command'], ['coordination', 'agent', 'list'], ['check', 'run'],
      ['status', 'unexpected'], ['status', '--limit', 'oops'], ['signal', 'publish', '--kind', 'fyi', '--kind', 'request'],
      ['status', '--unknown', 'true'], ['status', '--no-limit'], ['status', '--workspace'],
      ['database', 'consolidate'], ['hook', 'run', 'notify-deliver'],
    ]) expect((await executeAwarenessCli(['--db', context.database, ...args])).exitCode, args.join(' ')).toBe(1);
    expect(existsSync(context.database)).toBe(false);
    expect(await call('unknown')).toMatchObject({ exitCode: 1, payload: { error: expect.stringContaining('Unknown Awareness command') } });
    const unknown = await call('status', { typo: true });
    expect(unknown).toMatchObject({ exitCode: 1, payload: { known_flags: expect.arrayContaining(['--workspace']) } });
    expect(await call('task create', { run_id: 'wrong-lifecycle' })).toMatchObject({ exitCode: 1, payload: { error: expect.stringContaining('task claim') } });
  });

  it('serves on-demand guides, docs and schema input without initializing coordination state', async () => {
    for (const args of [[], ['schema', '--help'], ['history', 'capture', '--help', '--compact']]) {
      const result = await executeAwarenessCli(args);
      expect(result.exitCode).toBe(0); expect(result.text).toContain('npx @octocodeai/octocode-awareness');
    }
    expect((await call('guide')).text).toContain('<awareness>');
    expect(ok(await call('guide', { json: true }))).toBeTruthy();
    expect(ok(await call('instructions export', { format: 'json' })).instructions).toContain('<awareness>');
    expect((await call('instructions export', { format: 'agents-md' })).text).toContain('Awareness');
    expect((await call('instructions export')).text).toContain('<awareness>');
    ok(await call('docs list'));
    const input = join(root, 'recall.json'); writeFileSync(input, '{"query":"parser"}');
    ok(await executeAwarenessCli(['schema', 'validate', 'memory_recall', input]));
    expect((await executeAwarenessCli(['schema', 'validate', 'memory_recall', '-'])).exitCode).not.toBe(0);
    expect(existsSync(context.database)).toBe(false);
  });

  it('previews, installs and checks host configuration through the same native API', async () => {
    ok(await call('skill install', { platform: 'shared', project_dir: root, dry_run: true }));
    expect(existsSync(join(root, '.agents'))).toBe(false);
    ok(await call('config init', { verification_gate: false }));
    ok(await call('config validate')); ok(await call('config show'));
    const params = { host: 'codex', project_dir: root, profile: 'full' };
    const preview = ok(await call('hooks install', { ...params, dry_run: true }));
    expect(preview.action).toBe('dry-run'); expect(existsSync(join(root, '.codex', 'hooks.json'))).toBe(false);
    ok(await call('hooks install', params));
    const checked = ok(await call('hooks check', { ...params, strict: true }));
    expect(checked.ok).toBe(true);
    const source = readFileSync(join(root, '.codex', 'hooks.json'), 'utf8');
    ok(await call('hooks remove', { ...params, dry_run: true }));
    expect(readFileSync(join(root, '.codex', 'hooks.json'), 'utf8')).toBe(source);
    ok(await call('hooks remove', params));
    expect((await call('hooks check', { ...params, strict: true })).exitCode).toBe(2);
  });

  it('accepts external hook stdin, captures diagnostics and rejects store overrides', async () => {
    const payload = { cwd: root, session_id: 'external-session', hook_event_name: 'SessionStart' };
    const command = ['hook', 'run', 'notify-deliver'];
    ok(await executeAwarenessCli(command, { readStdin: async () => JSON.stringify(payload) }));
    expect((await executeAwarenessCli(command, { readStdin: async () => 'invalid' })).exitCode).toBe(1);
    const anonymous = await executeAwarenessCommand({ command: 'hook run', params: { event: 'notify-deliver', payload: { cwd: root } } });
    expect(anonymous).toMatchObject({ exitCode: 1, diagnostics: [expect.stringContaining('hook identity error')] });
    expect((await call('hook run', { event: 'notify-deliver', payload })).exitCode).toBe(1);
    ok(await call('hooks pre-edit', { host: 'codex', event_json: '{}' }));
    ok(await executeAwarenessCli(['hooks', 'pre-edit', '--workspace', root, '--db', context.database, '--agent-id', 'owner'], { readStdin: async () => '{}' }));
  });

  it('copies a current ledger through the API while preserving the source and rejecting overwrite', async () => {
    ok(await call('agent register', { agent_name: 'Owner' }));
    const destination = join(root, 'copied.sqlite3');
    const params = { source: context.database, destination };
    const before = readFileSync(context.database);
    ok(await call('database consolidate', { ...params, dry_run: true }));
    expect(existsSync(destination)).toBe(false);
    ok(await call('database consolidate', params));
    expect(readFileSync(context.database)).toEqual(before);
    expect((await call('database consolidate', params)).exitCode).toBe(1);
    const peers = ok(await executeAwarenessCommand({ command: 'agent list' }, { ...context, database: destination }));
    expect(peers.agents).toEqual(expect.arrayContaining([expect.objectContaining({ agent_id: 'owner' })]));
  });

  it('keeps host handoffs and retention previews on the canonical ledger', async () => {
    ok(await call('handoff add', { summary: 'Continue parser verification', file: ['a.ts', 'b.ts'] }));
    expect(JSON.stringify(ok(await call('handoff list')))).toContain('Continue parser verification');
    for (const older_than of ['1d', '1.5h', '60000']) ok(await call('memory prune', { older_than }));
    ok(await call('memory prune', { older_than: '1d', confirm: true }));
    expect((await call('memory prune', { older_than: 'never' })).exitCode).toBe(1);
    expect((await call('agent leave')).exitCode).toBe(0);
  });
});
