/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const SKILL_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/awareness.mjs');
const SOURCE_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../bin/awareness.ts');
const TSX_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../../../node_modules/tsx/dist/cli.mjs');
const NODE = process.execPath;
// ─── Helpers ─────────────────────────────────────────────────────────────────
function mktemp(): string {
    return mkdtempSync(join(tmpdir(), 'oc-cli-test-'));
}
interface RunResult {
    status: number;
    stdout: string;
    stderr: string;
    parsed: Record<string, unknown> | null;
}
function run(dbPath: string, args: string[], opts: {
    cwd?: string;
} = {}): RunResult {
    const result = spawnSync(NODE, [SCRIPT, '--db', dbPath, ...args], {
        cwd: opts.cwd ?? process.cwd(),
        encoding: 'utf8',
        // repo inject / heavy CLI paths can exceed 10s on cold machines
        timeout: 30000,
    });
    let parsed: Record<string, unknown> | null = null;
    try {
        parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    }
    catch { /* non-JSON */ }
    return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}
function runSource(args: string[], opts: {
    cwd?: string;
    env?: NodeJS.ProcessEnv;
} = {}): RunResult {
    const result = spawnSync(NODE, [TSX_SCRIPT, SOURCE_SCRIPT, ...args], {
        cwd: opts.cwd ?? process.cwd(),
        env: opts.env ?? process.env,
        encoding: 'utf8',
        timeout: 30000,
    });
    let parsed: Record<string, unknown> | null = null;
    try {
        parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    }
    catch { /* non-JSON */ }
    return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}
function ok(dbPath: string, args: string[], opts: {
    cwd?: string;
} = {}): Record<string, unknown> {
    const r = run(dbPath, args, opts);
    expect(r.status, `expected exit 0 for ${args[0]}: stderr=${r.stderr} stdout=${r.stdout}`).toBe(0);
    expect(r.parsed?.['ok'], `expected ok:true for ${args[0]}: ${r.stdout}`).not.toBe(false);
    return r.parsed!;
}
function fail(dbPath: string, args: string[], expectedStatus = 1): Record<string, unknown> | null {
    const r = run(dbPath, args);
    expect(r.status, `expected exit ${expectedStatus} for ${args[0]}: stdout=${r.stdout}`).toBe(expectedStatus);
    return r.parsed;
}

// ─── CLI ─────────────────────────────────────────────────────────────────────

describe('CLI', () => {
it('lock acquire help omits rejected --lock-type flag', () => {
    const help = spawnSync(NODE, [SCRIPT, 'lock', 'acquire', '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    expect(help.stdout).not.toMatch(/\[--lock-type/);
    expect(help.stdout).toContain('exclusive protection');

    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const rejected = fail(db, [
        'lock', 'acquire',
        '--agent-id', 'agent-a',
        '--target-file', join(dir, 'x.ts'),
        '--rationale', 'probe',
        '--test-plan', 'none',
        '--lock-type', 'EXCLUSIVE',
        '--compact',
      ]);
      expect(String(rejected?.['error'] ?? '')).toContain('unknown flag(s): --lock-type');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('focused help exposes skill-prescribed reflection and projection flags', () => {
    const reflect = runSource(['reflect', 'record', '--help']);
    expect(reflect.status).toBe(0);
    for (const flag of ['--fix-harness', '--duo', '--eval-failure-json', '--worked', '--didnt-work', '--judgment-note', '--allow-similar', '--artifact', '--repo', '--ref']) {
      expect(reflect.stdout).toContain(flag);
    }

    const refinement = runSource(['refinement', 'set', '--help']);
    expect(refinement.status).toBe(0);
    expect(refinement.stdout).toContain('update: --refinement-id <id> --state');

    const task = runSource(['task', 'create', '--help']);
    expect(task.status).toBe(0);
    for (const token of ['show:', 'depend:', '--test-plan', '--priority', '--lease-minutes', '--workspace']) {
      expect(task.stdout).toContain(token);
    }

    const query = runSource(['query', '--help']);
    expect(query.status).toBe(0);
    for (const flag of ['--query', '--limit', '--agent-id', '--state', '--include-bodies']) {
      expect(query.stdout).toContain(flag);
    }

    const hook = runSource(['hook', 'run', '--help']);
    expect(hook.status).toBe(0);
    expect(hook.stdout).toContain('session-compact');
    expect(hook.stdout).toContain('intentionally rejects --db');

    const inject = runSource(['repo', 'inject', '--help']);
    expect(inject.status).toBe(0);
    expect(inject.stdout).toContain('--prune-orphans');
  });
it('every command in schema commands has focused help or is schema/hook utility', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    const result = spawnSync(NODE, [schemaScript, 'commands', '--all', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    const parsed = JSON.parse(result.stdout) as { commands: Array<{ command: string }> };
    const commandNames = parsed.commands.map((row) => row.command);
    expect(new Set(commandNames).size).toBe(commandNames.length);
    const utilityPrefixes = ['schema ', 'hook run', 'hooks '];
    for (const row of parsed.commands) {
      if (utilityPrefixes.some((prefix) => row.command.startsWith(prefix))) continue;
      const help = spawnSync(NODE, [SCRIPT, ...row.command.split(' '), '--help'], { encoding: 'utf8', timeout: 5000 });
      expect(help.status, `${row.command} --help failed`).toBe(0);
      expect(help.stdout, `${row.command} help should mention its command`).toContain(row.command.split(' ')[0]);
      expect(help.stdout, `${row.command} help should not fall back to top-level help`).not.toContain('agent map: octocode-awareness schema commands --compact');
      expect(help.stdout, `${row.command} help should show schema or example`).toMatch(/schema:|example:/);
    }
  });
it('generated skill CLI delegates schema commands to its sibling schema script', () => {
    expect(existsSync(SKILL_SCRIPT), 'generated awareness.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [SKILL_SCRIPT, 'schema', 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status, schema.stderr || schema.stdout).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    expect(listed).toContain('memory_recall');
    expect(listed).not.toEqual(expect.arrayContaining([
      'tell_memory', 'get_memory', 'repo_inject', 'pre_flight_intent',
      'wait_for_lock', 'prune_stale_locks', 'release_file_lock', 'audit_unverified',
    ]));
  });
it('every listed schema resolves and its example validates', { timeout: 30_000 }, () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    for (const key of listed) {
      const jsonSchema = spawnSync(NODE, [schemaScript, 'json-schema', key], { encoding: 'utf8', timeout: 5000 });
      expect(jsonSchema.status, `${key} json-schema failed: ${jsonSchema.stderr || jsonSchema.stdout}`).toBe(0);

      const example = spawnSync(NODE, [schemaScript, 'example', key], { encoding: 'utf8', timeout: 5000 });
      expect(example.status, `${key} example failed: ${example.stderr || example.stdout}`).toBe(0);
      expect(() => JSON.parse(example.stdout)).not.toThrow();

      const validated = spawnSync(NODE, [schemaScript, 'validate', key, '-'], {
        encoding: 'utf8',
        input: example.stdout,
        timeout: 5000,
      });
      expect(validated.status, `${key} example should validate: ${validated.stdout || validated.stderr}`).toBe(0);
    }
  });
it('memory label schema stays aligned with runtime labels', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'memory_record'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    expect(schema.stdout).toContain('"EXPERIENCE"');
    expect(schema.stdout).toContain('"OVERRIDE"');
  });
it('schema exposes only implemented memory recall options', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'memory_recall'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    const sortSchema = parsed.properties['sort'];
    expect(sortSchema).toBeDefined();
    expect(sortSchema?.['enum']).toEqual(['smart', 'score', 'importance', 'recent', 'accessed']);
    expect(parsed.properties).not.toHaveProperty('no_decay');
    expect(parsed.properties).not.toHaveProperty('half_life');
  });
it('attend schema exposes the agent identity used by CLI and skill guidance', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'attend'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(parsed.properties).toHaveProperty('agent_id');
  });
it('schema aligns pre-flight ttl and retry contract with CLI/runtime', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'lock_acquire'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    const ttlSchema = parsed.properties['ttl_minutes'];
    const ttlSecondsSchema = parsed.properties['ttl_seconds'];
    const waitSchema = parsed.properties['wait_seconds'];
    const retrySchema = parsed.properties['retry_interval'];
    expect(ttlSchema).toBeDefined();
    expect(ttlSecondsSchema).toBeDefined();
    expect(waitSchema).toBeDefined();
    expect(retrySchema).toBeDefined();
    expect(ttlSchema?.['default']).toBe(10);
    expect(ttlSchema?.['maximum']).toBe(10);
    expect(ttlSecondsSchema?.['maximum']).toBe(600);
    expect(waitSchema?.['maximum']).toBe(3600);
    expect(retrySchema?.['default']).toBe(5);
    expect(retrySchema?.['maximum']).toBe(300);
  });
it('schema covers runtime drift cases for verify, audit, and handoff refinements', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);

    const verify = spawnSync(NODE, [schemaScript, 'json-schema', 'verify'], { encoding: 'utf8', timeout: 5000 });
    expect(verify.status).toBe(0);
    const verifySchema = JSON.parse(verify.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(verifySchema.properties['status']?.['enum']).toEqual(['SUCCESS', 'FAILED']);

    const audit = spawnSync(NODE, [schemaScript, 'json-schema', 'verify_audit'], { encoding: 'utf8', timeout: 5000 });
    expect(audit.status).toBe(0);
    expect(audit.stdout).not.toContain('"abandon"');

    const refinement = spawnSync(NODE, [schemaScript, 'json-schema', 'refinement'], { encoding: 'utf8', timeout: 5000 });
    expect(refinement.status).toBe(0);
    const refinementSchema = JSON.parse(refinement.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(refinementSchema.properties['quality']?.['enum']).toEqual(['good', 'bad', 'handoff', 'instructions']);
  });
it('schema exposes implemented forget scope filters', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'forget_memory'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(parsed.properties['workspace_path']).toBeDefined();
    expect(parsed.properties['artifact']).toBeDefined();
    expect(parsed.properties['repo']).toBeDefined();
    expect(parsed.properties['ref']).toBeDefined();
  });
it('supports canonical noun/verb CLI commands', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, ['maintenance', 'init']);
      const record = ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--task-context', 'canonical command',
        '--observation', 'noun verb memory works',
        '--importance', '6',
      ]);
      expect((record['memory'] as Record<string, unknown>)['memory_id']).toBeTruthy();

      const recall = ok(db, ['memory', 'recall', '--query', 'noun verb memory', '--limit', '1']);
      expect((recall['memories'] as unknown[]).length).toBeGreaterThan(0);
    } finally { rmSync(dir, { recursive: true }); }
  });

});

// ─── query markdown ───────────────────────────────────────────────────────────

describe('query memories markdown', () => {
  it('includes provenance references and failure signatures', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'memory', 'record', '--agent-id', 'a',
        '--task-context', 'indexed provenance',
        '--observation', 'memory index should preserve source context',
        '--importance', '8',
        '--label', 'GOTCHA',
        '--workspace', dir,
        '--tag', 'index',
        '--reference', 'file:packages/octocode-awareness/src/memory.ts',
        '--reference', 'pr:owner/repo#123',
        '--failure-signature', 'mechanism:index|cause:lost-provenance',
      ]);

      const result = run(db, ['query', 'memories', '--workspace', dir, '--format', 'markdown']);
      expect(result.status).toBe(0);
      expect(result.stdout).toContain('indexed provenance');
      expect(result.stdout).toContain('memory index should preserve source context');
      expect(result.stdout).toContain('mechanism:index|cause:lost-provenance');
    } finally { rmSync(dir, { recursive: true }); }
  });
});
