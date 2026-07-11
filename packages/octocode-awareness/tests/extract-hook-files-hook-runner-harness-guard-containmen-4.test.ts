import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const NODE = process.execPath;
function runScript(script: string, args: string[], payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
    return spawnSync(NODE, [script, ...args], {
        input: JSON.stringify(payload),
        encoding: 'utf8',
        timeout: 5000,
        cwd,
        env: { ...process.env, ...env },
    });
}

describe('hook-runner harness-guard containment', () => {
  // Exercises the harness-guard CLI dispatch end-to-end (payload extraction +
  // path resolution + containment check), which previously had no integration
  // test — hiding a relative-path traversal bypass of the self-edit gate.
  function guard(skillRoot: string | undefined, files: string[], cwd: string, extraEnv: Record<string, string | undefined> = {}) {
    return runScript(
      HOOK_RUNNER,
      ['harness-guard'],
      { tool_name: 'Edit', tool_input: { file_paths: files } },
      { OCTOCODE_SKILL_ROOT: skillRoot, OCTOCODE_ALLOW_HARNESS_APPLY: undefined, ...extraEnv },
      cwd,
    );
  }

  it('is a no-op when OCTOCODE_SKILL_ROOT is unset', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    try {
      expect(guard(undefined, ['SKILL.md'], tmp).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('allows edits resolving outside the skill root', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const project = join(tmp, 'project');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(project, { recursive: true });
    try {
      expect(guard(skillRoot, ['notes.txt'], project).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('gates absolute edits inside the skill root', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    mkdirSync(skillRoot, { recursive: true });
    try {
      const result = guard(skillRoot, [join(skillRoot, 'SKILL.md')], tmp);
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('OCTOCODE_ALLOW_HARNESS_APPLY');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('gates relative traversal edits that resolve inside the skill root', () => {
    // The bypass: cwd is outside the skill root, but a `../` path resolves back
    // in. A textual prefix check misses this; a normalized check must catch it.
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const project = join(tmp, 'project');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(project, { recursive: true });
    try {
      const result = guard(skillRoot, ['../skill/SKILL.md'], project);
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('OCTOCODE_ALLOW_HARNESS_APPLY');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('does not gate a sibling directory sharing the root name prefix', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const sibling = join(tmp, 'skill-sibling');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(sibling, { recursive: true });
    try {
      expect(guard(skillRoot, [join(sibling, 'x.ts')], tmp).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
