import { spawnSync } from 'node:child_process';
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const isomorphicGitVersion = (
  JSON.parse(
    readFileSync(
      join(
        dirname(createRequire(import.meta.url).resolve('isomorphic-git')),
        'package.json'
      ),
      'utf8'
    )
  ) as { version: string }
).version;
const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

interface Invocation {
  status: number;
  stdout: string;
  stderr: string;
  json: Record<string, unknown> | null;
}

function invoke(script: string, args: string[], cwd: string): Invocation {
  const result = spawnSync(process.execPath, [script, ...args], {
    cwd,
    encoding: 'utf8',
    timeout: 30_000,
    env: { ...process.env, PATH: '', OCTOCODE_HOME: join(cwd, '.home') },
  });
  let json: Record<string, unknown> | null = null;
  try { json = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* help is text */ }
  return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, json };
}

function expectOk(result: Invocation): Record<string, unknown> {
  expect(result.status, result.stderr || result.stdout).toBe(0);
  expect(result.json?.['ok']).not.toBe(false);
  return result.json!;
}

function isolatedArtifacts(withNative = false): { root: string; cli: string; runner: string; workspace: string; db: string } {
  const root = mkdtempSync(join(tmpdir(), 'awareness-history-cli-'));
  roots.push(root);
  const artifacts = join(root, 'published-assets');
  const workspace = join(root, 'workspace');
  mkdirSync(workspace);
  cpSync(resolve(packageRoot, 'out'), artifacts, { recursive: true });
  const cli = join(artifacts, 'octocode-awareness.js');
  const runner = join(artifacts, 'skill-scripts', 'awareness.mjs');
  chmodSync(cli, 0o755);
  chmodSync(runner, 0o755);
  expect(existsSync(join(artifacts, 'node_modules'))).toBe(false);
  if (withNative) {
    // Model an installed optional dependency using the real local package and
    // addon. The copied Awareness bundles remain separate from that package.
    const scope = join(root, 'node_modules', '@octocodeai');
    mkdirSync(scope, { recursive: true });
    const nativeRoot = dirname(createRequire(import.meta.url).resolve('@octocodeai/octocode-extension-rust'));
    symlinkSync(nativeRoot, join(scope, 'octocode-extension-rust'), process.platform === 'win32' ? 'junction' : 'dir');
  }
  return { root, cli, runner, workspace: realpathSync(workspace), db: join(root, 'awareness.sqlite3') };
}

describe('built local-history CLI contract', () => {
  it('runs strict before/after capture and lossless read continuations without system Git', () => {
    const { cli, workspace, db } = isolatedArtifacts(true);
    writeFileSync(join(workspace, 'a.bin'), Buffer.from([0, 1, 2, 3, 255]));
    const common = ['--db', db, 'history'];
    const before = expectOk(invoke(cli, [...common, 'capture', '--workspace', workspace, '--agent-id', 'cli-test', '--phase', 'before', '--operation-id', 'cli-edit', '--file', 'a.bin', '--compact'], workspace));
    expect(before['operation']).toMatchObject({ operation_id: 'cli-edit', status: 'open', outcome: 'unknown' });
    writeFileSync(join(workspace, 'a.bin'), Buffer.from([9, 8, 7]));
    const after = expectOk(invoke(cli, [...common, 'capture', '--workspace', workspace, '--agent-id', 'cli-test', '--phase', 'after', '--operation-id', 'cli-edit', '--outcome', 'success', '--compact'], workspace));
    expect(after['operation']).toMatchObject({ operation_id: 'cli-edit', status: 'complete', outcome: 'success' });

    const first = expectOk(invoke(cli, [...common, 'read', '--workspace', workspace, '--operation-id', 'cli-edit', '--file', 'a.bin', '--side', 'before', '--limit', '2', '--compact'], workspace));
    expect(first).toMatchObject({ encoding: 'base64', partial: true, offset: 0, total_bytes: 5 });
    const next = first['next'] as { command: string; args: Record<string, unknown>; argv: string[] };
    expect(next).toMatchObject({ command: 'history read', args: { workspace, operation_id: 'cli-edit', file: 'a.bin', side: 'before', offset: 2, limit: 2 } });
    expect(next.argv.slice(0, 3)).toEqual(['history', 'read', '--db']);
    expect(next.argv[3]).toBe(realpathSync(db));
    expect(next.argv.at(-1)).toBe('--compact');
    const pages = [first];
    let continuation: typeof next | null = next;
    while (continuation) {
      const page = expectOk(invoke(cli, continuation.argv, workspace));
      pages.push(page);
      continuation = page['next'] as typeof next | null;
    }
    const bytes = Buffer.concat(pages.map(page => Buffer.from(String(page['content']), 'base64')));
    expect(bytes).toEqual(Buffer.from([0, 1, 2, 3, 255]));
  });

  it('rejects explicit capture from both bundles when the optional native package is absent', () => {
    const { cli, runner, workspace, db } = isolatedArtifacts();
    writeFileSync(join(workspace, 'source.ts'), 'export const value = 1;');
    for (const script of [cli, runner]) {
      const result = invoke(script, ['--db', db, 'history', 'capture', '--workspace', workspace,
        '--agent-id', 'cli-test', '--phase', 'before', '--file', 'source.ts', '--compact'], workspace);
      expect(result.status).not.toBe(0);
      expect(`${result.stdout}\n${result.stderr}`).toContain('native filesystem is unavailable');
    }
  });

  it('keeps memory status read-only and runs both copied standalone bundles without sibling dependencies', () => {
    const { root, cli, runner, workspace } = isolatedArtifacts();
    const before = readdirSync(root).sort();
    for (const script of [cli, runner]) {
      const status = expectOk(invoke(script, ['--db', ':memory:', 'history', 'status', '--workspace', workspace, '--compact'], workspace));
      expect(status).toMatchObject({ available: false, initialized: false, disabled_reason: 'memory_database' });
      expect(status['backend']).toMatchObject({ name: 'isomorphic-git', version: isomorphicGitVersion, bundled: true, system_git_required: false });
    }
    expect(existsSync(join(workspace, '.octocode'))).toBe(false);
    expect(readdirSync(root).sort()).toEqual(before);
  });

  it('exposes focused help and a discriminated capture schema from both build products', () => {
    const { cli, runner, workspace } = isolatedArtifacts();
    for (const script of [cli, runner]) {
      const help = invoke(script, ['history', 'capture', '--help'], workspace);
      expect(help.status, help.stderr || help.stdout).toBe(0);
      expect(help.stdout).toContain('history capture [options]');
      for (const flag of ['--workspace', '--agent-id', '--phase', '--operation-id', '--file', '--outcome']) expect(help.stdout).toContain(flag);
      const schemaResult = invoke(script, ['schema', 'command', 'history', 'capture', '--compact'], workspace);
      expect(schemaResult.status, schemaResult.stderr || schemaResult.stdout).toBe(0);
      const schema = schemaResult.json as { oneOf: Array<{ properties: { phase: { const: string } }; required: string[]; additionalProperties: boolean }> };
      expect(schema.oneOf.map(branch => branch.properties.phase.const)).toEqual(['before', 'after']);
      expect(schema.oneOf[0]?.required).toEqual(expect.arrayContaining(['workspace', 'agent_id', 'phase', 'file']));
      expect(schema.oneOf[1]?.required).toEqual(expect.arrayContaining(['workspace', 'agent_id', 'phase', 'operation_id', 'outcome']));
      expect(schema.oneOf.every(branch => branch.additionalProperties === false)).toBe(true);
    }
  });
});
