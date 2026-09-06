/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync, readFileSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = resolve(PACKAGE_ROOT, 'out/octocode-awareness.js');
const SKILL_SCRIPT = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs');
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

// ─── maintenance self-test ────────────────────────────────────────────────────

describe('maintenance self-test', () => {
  it('all checks pass (uses in-memory DB)', () => {
    // maintenance self-test ignores --db flag, always uses :memory:
    const r = spawnSync(NODE, [SCRIPT, 'maintenance', 'self-test'], { encoding: 'utf8', timeout: 10000 });
    expect(r.status).toBe(0);
    const parsed = JSON.parse(r.stdout) as Record<string, unknown>;
    expect(parsed['ok']).toBe(true);
    const checks = parsed['checks'] as Record<string, boolean>;
    expect(checks['write']).toBe(true);
    expect(checks['fts_recall']).toBe(true);
    expect(checks['scoring']).toBe(true);
    expect(checks['refinement']).toBe(true);
  });
});

// ─── CLI ─────────────────────────────────────────────────────────────────────

describe('CLI', () => {
it('package metadata exposes the scoped public npx binary', () => {
    const packageJsonPath = resolve(dirname(fileURLToPath(import.meta.url)), '../package.json');
    const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as {
      name?: string;
      bin?: string | Record<string, string>;
      engines?: Record<string, string>;
      publishConfig?: Record<string, string>;
    };
    expect(pkg.name).toBe('@octocodeai/octocode-awareness');
    expect(pkg.publishConfig?.access).toBe('public');
    // Yarn 4 normalizes a single-entry bin object keyed by the unscoped package
    // name to string form on install; both forms name the binary octocode-awareness.
    const bin = typeof pkg.bin === 'string' ? { 'octocode-awareness': pkg.bin } : (pkg.bin ?? {});
    expect(bin['octocode-awareness']).toBe('./out/octocode-awareness.js');
    expect(pkg.engines?.['node']).toBe('>=22.13.0');
    expect(bin).not.toHaveProperty('awareness');
  });
it('--help exits 0', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('memory record');
    expect(r.stdout).toContain('octocode-awareness');
    expect(r.stdout).toContain('AGENT LOOP');
    expect(r.stdout).toContain('ROUTINE LOOP');
    expect(r.stdout).toContain('DEFAULT POLICY');
    expect(r.stdout).toMatch(/database\s+→ \$OCTOCODE_HOME\/awareness\/awareness.sqlite3/);
    expect(r.stdout).toMatch(/repo override\s+→ <workspace>\/\.octocode\/awareness.sqlite3/);
    expect(r.stdout).toContain('schema commands --compact');
    expect(r.stdout).toContain('Preview and ask immediately before a real install');
    expect(r.stdout).toContain('Pi uses native events');
    expect(r.stdout).not.toContain('octocode-skills');
    expect(r.stdout).toContain('out/skills');
    expect(r.stdout).toContain('schema commands --compact');
    expect(Buffer.byteLength(r.stdout, 'utf8')).toBeLessThanOrEqual(3072);
    expect(r.stdout).not.toContain('tell-memory');
    expect(r.stdout).not.toContain('get-memory');
    expect(r.stdout).not.toContain('<awareness-package>');
  });
it('installed skill help resolves sibling bundled skills from the skill root', () => {
    const r = spawnSync(NODE, [SKILL_SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    const skillsRoot = resolve(dirname(SKILL_SCRIPT), '..', '..');
    expect(r.stdout).toContain(skillsRoot);
    expect(existsSync(resolve(skillsRoot, 'octocode-awareness', 'SKILL.md'))).toBe(true);
  });
it('no command prints the agent-instructions discovery guide', () => {
    const r = spawnSync(NODE, [SCRIPT], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('ROUTINE LOOP');
    expect(r.stdout).toContain('AGENT LOOP');
    expect(Buffer.byteLength(r.stdout, 'utf8')).toBeLessThanOrEqual(3072);
    expect(r.stdout).not.toContain('<awareness-package>');
  });
it('--help --compact returns a short agent guide', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toContain('$OCTOCODE_HOME/awareness/awareness.sqlite3');
    expect(r.stdout).not.toContain('global agent database');
    expect(r.stdout).toMatch(/bundled-skills\(\d+\):/);
    expect(r.stdout).toContain('octocode-awareness');
    expect(r.stdout).toContain('out/skills');
    expect(r.stdout).toContain('docs list --compact');
    expect(r.stdout).toContain('schema commands --compact');
    expect(r.stdout).toContain('attend -> work start -> work end -> verify mark -> verify audit');
    expect(r.stdout).toContain('policy: $OCTOCODE_HOME/awareness/awareness.sqlite3, workspace-scoped rows, hooks=coordination');
    expect(r.stdout).toContain('attend|plan|task|work|verify|signal|memory|refinement');
    expect(r.stdout).toMatch(/exits: 0 ok/);
    expect(r.stdout).not.toContain('<awareness-package>');
    expect(r.stdout.split('\n').filter(Boolean).length).toBeLessThanOrEqual(10);
  });
it('resolves bundled skills when the CLI is invoked through an npx-style symlink', () => {
    const dir = mktemp();
    const linkedScript = join(dir, 'octocode-awareness');
    try {
      symlinkSync(SCRIPT, linkedScript);
      const r = spawnSync(NODE, [linkedScript, '--help', '--compact'], { encoding: 'utf8', timeout: 5000 });
      expect(r.status).toBe(0);
      expect(r.stdout).toContain('bundled-skills(1):');
      expect(r.stdout).toContain('out/skills');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
it('no command with --compact prints compact discovery instead of unknown-command JSON', () => {
    const r = spawnSync(NODE, [SCRIPT, '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toContain('$OCTOCODE_HOME/awareness/awareness.sqlite3');
    expect(r.stdout).not.toContain('global agent database');
    expect(r.stdout).toMatch(/bundled-skills\(\d+\):/);
    expect(r.stdout).toContain('out/skills');
    expect(r.stdout).not.toContain('<awareness-package>');
    expect(r.stdout).not.toContain('unknown command');
  });
it('unknown flag-only invocation still exits nonzero', () => {
    const r = spawnSync(NODE, [SCRIPT, '--bogus-flag', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(1);
    expect(r.stdout).toContain('unknown command');
  });
it('unknown command exits 1 with JSON error', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const r = run(db, ['totally-unknown-command-xyz']);
      expect(r.status).toBe(1);
      expect(r.parsed?.['error']).toContain('unknown command');
    } finally { rmSync(dir, { recursive: true }); }
  });
it('OCTOCODE_AWARENESS_COMPACT=1 env produces compact output', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const r = spawnSync(NODE, [SCRIPT, '--db', db, 'maintenance', 'init'], {
        encoding: 'utf8', timeout: 5000,
        env: { ...process.env, OCTOCODE_AWARENESS_COMPACT: '1' },
      });
      expect(r.status).toBe(0);
      expect(r.stdout.trim().split('\n')).toHaveLength(1);
    } finally { rmSync(dir, { recursive: true }); }
  });
it('--db after command works (extractGlobalDb is position-independent)', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const r = spawnSync(NODE, [SCRIPT, 'maintenance', 'init', '--db', db], { encoding: 'utf8', timeout: 5000 });
      expect(r.status).toBe(0);
      const parsed = JSON.parse(r.stdout) as Record<string, unknown>;
      expect(parsed['initialized']).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });
it('removed flat commands are plain unknown commands with no compatibility metadata', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const removedMemory = run(db, ['get-memory', '--query', 'sqlite']);
      expect(removedMemory.status).toBe(1);
      expect(removedMemory.parsed).not.toHaveProperty('replacement');
      expect(String(removedMemory.parsed?.['hint'])).toContain('canonical noun/verb commands only');

      const removedView = run(db, ['view', 'all']);
      expect(removedView.status).toBe(1);
      expect(removedView.parsed).not.toHaveProperty('replacement');
    } finally { rmSync(dir, { recursive: true }); }
  });
it('schema list maps to canonical CLI commands', () => {
    const schemaScript = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs');
    expect(existsSync(schemaScript), 'generated awareness.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'schema', 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    const catalog = spawnSync(NODE, [SCRIPT, 'schema', 'commands', '--all', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(catalog.status).toBe(0);
    const rows = JSON.parse(catalog.stdout).commands as Array<{ command: string; schema: string | null }>;
    const commands = Object.fromEntries(rows.filter(row => row.schema).reverse().map(row => [row.schema, row.command]));
    const unsupported = ['stats', 'embed_index', 'harness_apply', 'memory_export', 'memory_import', 'memory_index', 'view', 'notify', 'notify_query', 'notify_resolve', 'notify_prune', 'status'];
    expect(listed).not.toEqual(expect.arrayContaining(unsupported));
    expect(listed).toEqual(expect.arrayContaining([
      'session_capture',
      'mine_weakness',
      'digest',
      'doc_staleness',
      'docs_catalog',
      'workspace_status',
      'export_harness',
      'verify_audit',
    ]));
    for (const key of listed) {
      const command = commands[key];
      expect(command, `${key} should have canonical command mapping`).toBeTruthy();
      const focused = spawnSync(NODE, [SCRIPT, ...command!.split(' '), '--help'], { encoding: 'utf8', timeout: 5000 });
      expect(focused.status, `${command} help failed`).toBe(0);
      expect(focused.stdout, `${key} should map to CLI command ${command}`).toContain(command!);
    }
  });
  it('schema commands is grouped and core-first for agents', () => {
    const schemaScript = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs');
    const result = spawnSync(NODE, [schemaScript, 'schema', 'commands', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    expect(result.stdout.trim().split('\n')).toHaveLength(1);
    const parsed = JSON.parse(result.stdout) as {
      ok: boolean;
      hint: string;
      commands: { core: Record<string, string[]>; advanced: Record<string, string[]> };
    };
    expect(parsed.ok).toBe(true);
    expect(parsed.hint).toContain('minimum agent loop');
    expect(parsed.hint).toContain('Follow attend.next');
    expect(parsed.commands.core.attend).toEqual(['<direct>']);
    expect(parsed.commands.core.verify).toEqual(expect.arrayContaining(['audit', 'mark']));
    expect(parsed.commands.core.verify).not.toContain('<direct>');
    expect(parsed.commands.core).not.toHaveProperty('setup');
    expect(parsed.commands.core).not.toHaveProperty('next');
    expect(parsed.commands.core).not.toHaveProperty('inspect');
    expect(parsed.commands.core).not.toHaveProperty('close');
    expect(parsed.commands.core.plan).toEqual(expect.arrayContaining(['create', 'status']));
    expect(parsed.commands.core.task).toContain('claim');
    expect(parsed.commands.core).not.toHaveProperty('wiki');
    expect(parsed.commands.advanced.lock).toEqual(expect.arrayContaining(['acquire', 'release']));
    expect(Buffer.byteLength(result.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);
  });

  it('schema command supports direct noun forms and rejects the removed run placeholder', () => {
    const schemaScript = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs');
    const direct = spawnSync(NODE, [schemaScript, 'schema', 'command', 'attend', '--compact'], { encoding: 'utf8', timeout: 5000 });
    const legacy = spawnSync(NODE, [schemaScript, 'schema', 'command', 'attend', 'run', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(direct.status, direct.stderr || direct.stdout).toBe(0);
    expect(legacy.status).toBe(1);
    const directSchema = JSON.parse(direct.stdout) as { 'x-cli-command': string; properties: Record<string, unknown> };
    const legacyError = JSON.parse(legacy.stdout) as { error: string };
    expect(directSchema['x-cli-command']).toBe('attend');
    expect(legacyError.error).toContain('Unknown or schema-less CLI command: attend run');
    expect(directSchema.properties).toHaveProperty('query');
  });
  it('schema commands --examples restores recipe lines', () => {
    const schemaScript = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs');
    const result = spawnSync(NODE, [schemaScript, 'schema', 'commands', '--all', '--examples', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    const parsed = JSON.parse(result.stdout) as {
      commands: Array<{ command: string; use: string; example: string }>;
    };
    expect(parsed.commands.length).toBeGreaterThan(10);
    for (const row of parsed.commands) {
      expect(row.example).toContain('octocode-awareness ');
    }
    const lockRelease = parsed.commands.find((row) => row.command === 'lock release');
    expect(lockRelease?.example).toMatch(/--status PENDING/);
    expect(lockRelease?.example).not.toMatch(/--status ACTIVE/);
    const taskCreate = parsed.commands.find((row) => row.command === 'task create');
    expect(taskCreate?.example).toContain('--acceptance');
    const taskSubmit = parsed.commands.find((row) => row.command === 'task submit');
    expect(taskSubmit?.example).toContain('ready for verification');
    expect(taskSubmit?.example).not.toContain('tests pass');
    const workStart = parsed.commands.find((row) => row.command === 'work start');
    expect(workStart?.example).toContain('--workspace');
    const workTouch = parsed.commands.find((row) => row.command === 'work touch');
    expect(workTouch?.use).toMatch(/refresh.*already-declared.*start --run-id.*add files/i);
  });

});
