import fs from 'node:fs';
import path from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fingerprint, projectConfigFiles, projectTrustNow, saveDecision } from '../src/shared/trust.js';
import { repo, tmp } from './helpers.js';

beforeEach(() => {
  vi.stubEnv('OCTOCODE_HOME', tmp('octocode-trust-home-'));
  vi.stubEnv('PI_CODING_AGENT_DIR', tmp('octocode-trust-agent-'));
  vi.stubEnv('OCTOCODE_SUBAGENT', undefined);
  vi.stubEnv('OCTOCODE_TRUST_ROOT', undefined);
});

const ctx = (cwd: string) => ({ cwd, hasUI: false, isProjectTrusted: () => true });

function project(file: string, text: string): string {
  const dir = repo('octocode-trust-memo-');
  fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
  fs.writeFileSync(path.join(dir, file), text);
  return dir;
}

describe('trust memo and hook execution', () => {
  it('never answers `true` from the memo for a project whose hooks start commands', () => {
    const cwd = project('.codex/hooks.json', JSON.stringify({ hooks: { PreToolUse: [{ hooks: [{ type: 'command', command: 'echo a' }] }] } }));
    saveDecision(cwd, fingerprint(projectConfigFiles(cwd)), true);
    const reads = vi.spyOn(fs, 'readFileSync');
    try {
      expect(projectTrustNow(ctx(cwd))).toBe(true);
      reads.mockClear();
      expect(projectTrustNow(ctx(cwd))).toBe(true);
      // Computed afresh: the hooks file was read again.
      expect(reads.mock.calls.filter(([file]) => String(file).endsWith('hooks.json')).length).toBeGreaterThan(0);
    } finally {
      reads.mockRestore();
    }
  });

  it('catches a same-size, same-mtime rewrite of a trusted hook within the memo window', () => {
    const cwd = project('.codex/hooks.json', JSON.stringify({ hooks: { PreToolUse: [{ hooks: [{ type: 'command', command: 'echo a' }] }] } }));
    const file = path.join(cwd, '.codex', 'hooks.json');
    saveDecision(cwd, fingerprint(projectConfigFiles(cwd)), true);
    expect(projectTrustNow(ctx(cwd))).toBe(true);
    const { atime, mtime } = fs.statSync(file);
    fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('echo a', 'echo b'));
    fs.utimesSync(file, atime, mtime);
    expect(projectTrustNow(ctx(cwd))).toBeUndefined();
  });

  it('still memoizes a project without hook commands', () => {
    const cwd = project('.claude/skills/x/SKILL.md', '---\nname: x\n---\nbody');
    saveDecision(cwd, fingerprint(projectConfigFiles(cwd)), true);
    expect(projectTrustNow(ctx(cwd))).toBe(true);
    const reads = vi.spyOn(fs, 'readFileSync');
    try {
      expect(projectTrustNow(ctx(cwd))).toBe(true);
      expect(reads.mock.calls.filter(([file]) => String(file).endsWith('SKILL.md'))).toHaveLength(0);
    } finally {
      reads.mockRestore();
    }
  });
});
