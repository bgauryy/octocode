import { describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { evaluateHarnessGuard, extractPiWriteTargetPaths } from '../src/pi-hooks.js';
function gitRepoOnBranch(branch: string) {
    const dir = mkdtempSync(join(tmpdir(), 'oc-guard-repo-'));
    const git = (...args: string[]) => spawnSync('git', ['-C', dir, ...args], { encoding: 'utf8' });
    git('init', '-q');
    git('config', 'user.email', 't@t');
    git('config', 'user.name', 't');
    git('commit', '-q', '--allow-empty', '-m', 'init');
    git('branch', '-M', branch);
    return { dir, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

// evaluateHarnessGuard is the single source of truth shared by the Pi bridge and
// the shell hook runner (bin/hook-runner.ts), so both vendors gate identically.
describe('evaluateHarnessGuard', () => {
  const base = { env: {} as NodeJS.ProcessEnv };

  it('is a no-op when skillRoot is unset', () => {
    expect(evaluateHarnessGuard({ targetFiles: ['a.ts'], skillRoot: null, cwd: '/tmp', ...base })).toBeNull();
  });

  it('is a no-op for a target resolving outside the skill root', () => {
    const repo = gitRepoOnBranch('feature-x');
    try {
      expect(evaluateHarnessGuard({ targetFiles: ['/tmp/elsewhere.ts'], skillRoot: repo.dir, cwd: '/tmp', ...base })).toBeNull();
    } finally { repo.cleanup(); }
  });

  it('blocks an in-skill edit without OCTOCODE_ALLOW_HARNESS_APPLY', () => {
    const repo = gitRepoOnBranch('feature-x');
    try {
      const reason = evaluateHarnessGuard({ targetFiles: [join(repo.dir, 'SKILL.md')], skillRoot: repo.dir, cwd: repo.dir, env: {} });
      expect(reason).toContain('editing the skill itself is gated');
    } finally { repo.cleanup(); }
  });

  it('allows an approved in-skill edit on a dedicated branch', () => {
    const repo = gitRepoOnBranch('feature-x');
    try {
      const reason = evaluateHarnessGuard({ targetFiles: [join(repo.dir, 'SKILL.md')], skillRoot: repo.dir, cwd: repo.dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1' } });
      expect(reason).toBeNull();
    } finally { repo.cleanup(); }
  });

  it('blocks even when approved if the skill root is on main/master', () => {
    const repo = gitRepoOnBranch('main');
    try {
      const reason = evaluateHarnessGuard({ targetFiles: [join(repo.dir, 'SKILL.md')], skillRoot: repo.dir, cwd: repo.dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1' } });
      expect(reason).toContain('never allowed on main');
    } finally { repo.cleanup(); }
  });

  it('requires OCTOCODE_HARNESS_BRANCH_OK for a non-repo skill root', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-guard-norepo-'));
    try {
      const blocked = evaluateHarnessGuard({ targetFiles: [join(dir, 'SKILL.md')], skillRoot: dir, cwd: dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1' } });
      expect(blocked).toContain('cannot confirm a dedicated git branch');
      const allowed = evaluateHarnessGuard({ targetFiles: [join(dir, 'SKILL.md')], skillRoot: dir, cwd: dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1', OCTOCODE_HARNESS_BRANCH_OK: '1' } });
      expect(allowed).toBeNull();
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});

describe('extractPiWriteTargetPaths', () => {
  it('extracts Pi write/edit tool input shapes', () => {
    expect(extractPiWriteTargetPaths('write', { path: 'src/a.ts' })).toEqual(['src/a.ts']);
    expect(extractPiWriteTargetPaths('edit', { file_path: 'src/b.ts', filePaths: ['src/c.ts', 'src/b.ts'] })).toEqual([
      'src/b.ts',
      'src/c.ts',
    ]);
    expect(extractPiWriteTargetPaths('edit', {
      queries: [
        { path: 'src/d.ts' },
        { file_path: 'src/e.ts', filePaths: ['src/f.ts', 'src/d.ts'] },
      ],
    })).toEqual(['src/d.ts', 'src/e.ts', 'src/f.ts']);
  });

  it('extracts apply_patch file paths from command payloads', () => {
    expect(extractPiWriteTargetPaths('bash', {
      command: ['*** Begin Patch', '*** Update File: src/a.ts', '*** Move to: src/b.ts', '*** End Patch'].join('\n'),
    })).toEqual(['src/a.ts', 'src/b.ts']);
  });
});
