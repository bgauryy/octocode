import { execFile } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';

const execFileAsync = promisify(execFile);
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cli = resolve(packageRoot, 'out/octocode-awareness.js');
const hostApi = pathToFileURL(resolve(packageRoot, 'out/host-api.js')).href;
const roots: string[] = [];

const CAPTURE = `
const [moduleUrl, database, workspace, agentId, operationId] = process.argv.slice(1);
const { createAwarenessHost } = await import(moduleUrl);
const result = await createAwarenessHost({ database, workspace, agentId }).captureHistory({
  phase: 'before', operation_id: operationId, file: ['shared.bin'],
});
process.stdout.write(JSON.stringify(result));
`;

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function args(db: string, values: string[]): string[] {
  return [cli, '--db', db, ...values, '--compact'];
}

function parse(stdout: string): Record<string, unknown> {
  return JSON.parse(stdout) as Record<string, unknown>;
}

describe('local-history process concurrency', () => {
  it('retains eight concurrent first captures in one fresh private store', async () => {
    const root = mkdtempSync(join(tmpdir(), 'awareness-history-process-'));
    roots.push(root);
    const workspace = join(root, 'workspace');
    mkdirSync(workspace);
    const canonicalWorkspace = realpathSync(workspace);
    const db = join(root, 'awareness.sqlite3');
    writeFileSync(join(workspace, 'shared.bin'), Buffer.from([0, 1, 2, 255]));
    const env = { ...process.env, PATH: '', OCTOCODE_HOME: join(root, '.home') };
    const captures = await Promise.allSettled(Array.from({ length: 8 }, async (_, index) => {
      const operationId = `process-${index}`;
      const result = await execFileAsync(process.execPath, [
        '--input-type=module', '--eval', CAPTURE, hostApi, db, workspace, `agent-${index}`, operationId,
      ], { cwd: workspace, env, timeout: 30_000, maxBuffer: 4 * 1024 * 1024 });
      const body = parse(result.stdout);
      expect(body['ok']).toBe(true);
      expect(body['operation']).toMatchObject({ operation_id: operationId, status: 'open' });
      return operationId;
    }));
    const failures = captures.filter(result => result.status === 'rejected');
    expect(failures, failures.map(result => result.status === 'rejected' ? `${String(result.reason)}\nstdout: ${String((result.reason as { stdout?: string }).stdout ?? '')}\nstderr: ${String((result.reason as { stderr?: string }).stderr ?? '')}` : '').join('\n')).toHaveLength(0);
    const operationIds = captures.map(result => result.status === 'fulfilled' ? result.value : '').filter(Boolean);

    const timelineResult = await execFileAsync(process.execPath, args(db, [
      'history', 'timeline', '--workspace', workspace, '--limit', '20',
    ]), { cwd: workspace, env, timeout: 30_000, maxBuffer: 4 * 1024 * 1024 });
    const timeline = parse(timelineResult.stdout);
    expect(timeline['ok']).toBe(true);
    expect(timeline['operations']).toHaveLength(8);
    expect(new Set((timeline['operations'] as Array<{ operation_id: string }>).map(row => row.operation_id))).toEqual(new Set(operationIds));

    const reads = await Promise.all(operationIds.map(operationId => execFileAsync(process.execPath, args(db, [
      'history', 'read', '--workspace', canonicalWorkspace, '--operation-id', operationId,
      '--file', 'shared.bin', '--side', 'before', '--limit', '1024',
    ]), { cwd: workspace, env, timeout: 30_000, maxBuffer: 4 * 1024 * 1024 })));
    for (const read of reads) {
      expect(parse(read.stdout)).toMatchObject({ ok: true, status: 'captured', content: Buffer.from([0, 1, 2, 255]).toString('base64'), partial: false });
    }
  });
});
