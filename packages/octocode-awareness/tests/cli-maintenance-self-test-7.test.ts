/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync, readFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const SKILL_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/awareness.mjs');
const NODE = process.execPath;
// Independent discovery (not imported from build.mjs) so CLI help assertions
// verify the real invariant — "every repo skill is enumerated" — rather than
// just agreeing with whatever the build happened to produce.
const REPO_SKILLS_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills');
const RETIRED_REPO_SKILLS = ['octocode-agent-communication', 'octocode-reflection'];
function discoverRepoSkillNames(): string[] {
    return readdirSync(REPO_SKILLS_ROOT, { withFileTypes: true })
        .filter((entry) => entry.isDirectory())
        .map((entry) => entry.name)
        .filter((name) => name !== 'scripts' && !RETIRED_REPO_SKILLS.includes(name))
        .filter((name) => existsSync(resolve(REPO_SKILLS_ROOT, name, 'SKILL.md')))
        .sort();
}
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
      bin?: Record<string, string>;
      engines?: Record<string, string>;
      publishConfig?: Record<string, string>;
    };
    expect(pkg.name).toBe('@octocodeai/octocode-awareness');
    expect(pkg.publishConfig?.access).toBe('public');
    expect(pkg.bin?.['octocode-awareness']).toBe('./out/octocode-awareness.js');
    expect(pkg.engines?.['node']).toBe('>=22.13.0');
    expect(pkg.bin ?? {}).not.toHaveProperty('awareness');
  });
it('--help exits 0', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('memory record');
    expect(r.stdout).toContain('<AGENT_INSTRUCTIONS>');
    expect(r.stdout).toContain('</AGENT_INSTRUCTIONS>');
    expect(r.stdout).toMatch(/BUNDLED SKILLS \(\d+\)/);
    expect(r.stdout).toContain('octocode-awareness');
    expect(r.stdout).toContain('octocode-skills');
    // Structural: every repo skill discovered independently must be enumerated,
    // and every enumerated skill path must resolve under out/skills/.
    const repoSkillNames = discoverRepoSkillNames();
    expect(repoSkillNames.length).toBeGreaterThanOrEqual(2);
    for (const name of repoSkillNames) {
      expect(r.stdout).toContain(name);
      expect(r.stdout).toContain(`out/skills/${name}`);
    }
    expect(r.stdout).toContain("Do not use a skill installer's registry/name lookup for these");
    expect(r.stdout).toContain('octocode-awareness schema commands --compact');
    expect(r.stdout).not.toContain('tell-memory');
    expect(r.stdout).not.toContain('get-memory');
    expect(r.stdout).not.toContain('<awareness-package>');
  });
it('installed skill help resolves sibling bundled skills from the skill root', () => {
    const r = spawnSync(NODE, [SKILL_SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    const skillsRoot = resolve(dirname(SKILL_SCRIPT), '..', '..');
    for (const name of discoverRepoSkillNames()) {
      const skillPath = resolve(skillsRoot, name);
      expect(r.stdout).toContain(skillPath);
      expect(existsSync(skillPath)).toBe(true);
    }
  });
it('no command prints the agent-instructions discovery guide', () => {
    const r = spawnSync(NODE, [SCRIPT], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('<AGENT_INSTRUCTIONS>');
    expect(r.stdout).toMatch(/BUNDLED SKILLS \(\d+\)/);
    for (const name of discoverRepoSkillNames()) {
      expect(r.stdout).toContain(`out/skills/${name}`);
    }
    expect(r.stdout).toContain('FIRST COMMANDS');
    expect(r.stdout).not.toContain('<awareness-package>');
  });
it('--help --compact returns a short agent guide', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toMatch(/bundled-skills\(\d+\):/);
    expect(r.stdout).toContain('out/skills');
    expect(r.stdout).toContain('schema commands --compact');
    expect(r.stdout).toContain('refinement set|get|delete');
    expect(r.stdout).toMatch(/exits 0 ok/);
    expect(r.stdout).not.toContain('<awareness-package>');
    expect(r.stdout.split('\n').filter(Boolean).length).toBeLessThanOrEqual(8);
  });
it('no command with --compact prints compact discovery instead of unknown-command JSON', () => {
    const r = spawnSync(NODE, [SCRIPT, '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
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
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    const help = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    const commands: Record<string, string> = {
      tell_memory: 'memory record',
      get_memory: 'memory recall',
      pre_flight_intent: 'lock acquire',
      wait_for_lock: 'lock wait',
      prune_stale_locks: 'lock prune',
      release_file_lock: 'lock release',
      audit_unverified: 'verify audit',
      verify: 'verify mark',
      forget_memory: 'memory forget',
      memory_lifecycle: 'memory archive',
      refinement: 'refinement set',
      refine_query: 'refinement get',
      refine_delete: 'refinement delete',
      agent_registry: 'agent register',
      agent_signal: 'signal publish',
      signal_prune: 'signal prune',
      workspace_status: 'workspace status',
      export_harness: 'reflect export-harness',
      developer_review: 'reflect developer-review',
      session_capture: 'session capture',
      mine_weakness: 'reflect mine-weakness',
      doc_staleness: 'docs staleness',
      docs_catalog: 'docs list',
      digest: 'maintenance digest',
      reflect: 'reflect record',
      attend: 'attend',
      query: 'query',
      repo_inject: 'repo inject',
      plan: 'plan create',
      task: 'task create',
      work: 'work start',
    };
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
      'audit_unverified',
    ]));
    for (const key of listed) {
      const command = commands[key];
      expect(command, `${key} should have canonical command mapping`).toBeTruthy();
      expect(help.stdout, `${key} should map to CLI command ${command}`).toContain(command);
    }
  });
it('schema commands is the compact command-to-schema map for agents', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    const result = spawnSync(NODE, [schemaScript, 'commands', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    expect(result.stdout.trim().split('\n')).toHaveLength(1);
    const parsed = JSON.parse(result.stdout) as {
      ok: boolean;
      commands: Array<{ command: string; schema: string | null; use?: string; example?: string }>;
    };
    expect(parsed.ok).toBe(true);
    expect(parsed.commands).toEqual(expect.arrayContaining([
      expect.objectContaining({ command: 'workspace status', schema: 'workspace_status' }),
      expect.objectContaining({ command: 'lock acquire', schema: 'pre_flight_intent' }),
      expect.objectContaining({ command: 'query files', schema: 'query' }),
      expect.objectContaining({ command: 'query all', schema: 'query' }),
      expect.objectContaining({ command: 'schema commands', schema: null }),
    ]));
    for (const row of parsed.commands) {
      expect(row.command).not.toMatch(/tell-memory|get-memory|notify|agent-registry|pre-flight/);
      expect(row).not.toHaveProperty('use');
      expect(row).not.toHaveProperty('example');
    }
    expect(Buffer.byteLength(result.stdout, 'utf8')).toBeLessThanOrEqual(5 * 1024);
  });
it('schema commands --examples restores recipe lines', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../../../skills/octocode-awareness/scripts/schema.mjs');
    const result = spawnSync(NODE, [schemaScript, 'commands', '--examples', '--compact'], { encoding: 'utf8', timeout: 5000 });
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
  });

});
