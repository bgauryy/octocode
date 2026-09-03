/**
 * TDD tests for subagents.ts lazy-evaluation contract.
 *
 * RED (before fix):
 *  - getExternalSkillDirs is not exported → TS/import error
 *  - resolveSubagentSkills is not exported → same
 *  - SUBAGENT_REGISTRY entries have eagerly-computed `skills` field
 *
 * GREEN (after fix):
 *  - Both functions exported and dynamic
 *  - SUBAGENT_REGISTRY uses extraSkillPaths instead of eager skills
 */
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  getExternalSkillDirs,
  resolveSubagentSkills,
  SUBAGENT_REGISTRY,
} from '../src/subagents.js';

// ─── getExternalSkillDirs ─────────────────────────────────────────────────────

describe('getExternalSkillDirs', () => {
  let tmpDir: string;
  let originalCwd: string;

  beforeEach(() => {
    originalCwd = process.cwd();
    // fs.realpathSync resolves macOS /var → /private/var symlink so path comparisons
    // against process.cwd() (which is also symlink-resolved) are consistent.
    tmpDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-external-skill-dirs-')));
  });

  afterEach(() => {
    process.chdir(originalCwd);
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('is a named export — not a frozen IIFE result', () => {
    expect(typeof getExternalSkillDirs).toBe('function');
  });

  it('includes <cwd>/.agents/skills relative to the CURRENT working directory', () => {
    process.chdir(tmpDir);
    const dirs = getExternalSkillDirs();
    expect(dirs).toContain(path.join(tmpDir, '.agents', 'skills'));
  });

  it('returns a different .agents/skills path when cwd changes between calls', () => {
    const dirsAtStart = getExternalSkillDirs();
    process.chdir(tmpDir);
    const dirsAfterChdir = getExternalSkillDirs();

    const cwdPathAtStart = dirsAtStart.find(d => d.endsWith(path.join('.agents', 'skills')));
    const cwdPathAfter = dirsAfterChdir.find(d => d.endsWith(path.join('.agents', 'skills')));

    // After chdir the cwd-relative path resolves under tmpDir, not the original cwd
    expect(cwdPathAtStart).not.toBe(cwdPathAfter);
    expect(cwdPathAfter).toBe(path.join(tmpDir, '.agents', 'skills'));
  });

  it('always includes the ~/.pi/agent/skills root when HOME is set', () => {
    const home = process.env.HOME;
    if (!home) return; // skip if HOME is unset in this env
    const dirs = getExternalSkillDirs();
    expect(dirs).toContain(path.join(home, '.pi', 'agent', 'skills'));
  });
});

// ─── SUBAGENT_REGISTRY — no eager skills field ────────────────────────────────

describe('SUBAGENT_REGISTRY', () => {
  it('does not have an eagerly-computed skills field on researcher', () => {
    // After fix: skills is removed from the registry; extraSkillPaths is used instead.
    expect(SUBAGENT_REGISTRY['researcher']).not.toHaveProperty('skills');
  });

  it('does not have an eagerly-computed skills field on planner', () => {
    expect(SUBAGENT_REGISTRY['planner']).not.toHaveProperty('skills');
  });

  it('does not have an eagerly-computed skills field on architect', () => {
    expect(SUBAGENT_REGISTRY['architect']).not.toHaveProperty('skills');
  });

  it('browser-agent uses extraSkillPaths (not skills) for its local skill dir', () => {
    const ba = SUBAGENT_REGISTRY['browser-agent'];
    expect(ba).not.toHaveProperty('skills');
    expect(ba).toHaveProperty('extraSkillPaths');
    expect(Array.isArray(ba.extraSkillPaths)).toBe(true);
  });

  it('typed subagents include write for parent-assigned durable handback artifacts', () => {
    expect(SUBAGENT_REGISTRY['browser-agent'].tools).toContain('write');
    expect(SUBAGENT_REGISTRY.researcher.tools).toContain('write');
    expect(SUBAGENT_REGISTRY.planner.tools).toContain('write');
    expect(SUBAGENT_REGISTRY.architect.tools).toContain('write');
  });

  it('researcher/planner prompts do not instruct unavailable bash tool use', () => {
    for (const name of ['researcher', 'planner'] as const) {
      expect(SUBAGENT_REGISTRY[name].tools).not.toContain('bash');
      const prompt = fs.readFileSync(SUBAGENT_REGISTRY[name].systemPromptPath!, 'utf8');
      expect(prompt).not.toMatch(/bash:\s*npx octocode skill/);
    }
  });
});

// ─── resolveSubagentSkills — lazy per-call resolution ─────────────────────────

describe('resolveSubagentSkills', () => {
  let tmpDir: string;
  let originalCwd: string;

  beforeEach(() => {
    originalCwd = process.cwd();
    tmpDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-resolve-skills-')));
  });

  afterEach(() => {
    process.chdir(originalCwd);
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('is a named export', () => {
    expect(typeof resolveSubagentSkills).toBe('function');
  });

  it('returns an array', () => {
    expect(Array.isArray(resolveSubagentSkills(SUBAGENT_REGISTRY['researcher']))).toBe(true);
  });

  it('discovers a skill installed in <cwd>/.agents/skills at CALL TIME (not import time)', () => {
    // Create a fake octocode-research skill dir AFTER module was already loaded
    const skillDir = path.join(tmpDir, '.agents', 'skills', 'octocode-research');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '# octocode-research\n');

    process.chdir(tmpDir);
    const skills = resolveSubagentSkills(SUBAGENT_REGISTRY['researcher']);
    expect(skills.some(s => s.includes('octocode-research'))).toBe(true);
  });

  it('does not include octocode-awareness from an external skill root because coordination is prompt-owned', () => {
    const skillDir = path.join(tmpDir, '.agents', 'skills', 'octocode-awareness');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '# octocode-awareness\n');

    process.chdir(tmpDir);
    const skills = resolveSubagentSkills(SUBAGENT_REGISTRY['architect']);
    expect(skills.some(s => path.basename(s) === 'octocode-awareness')).toBe(false);
  });

  it('does not include skills from a dir that no longer exists at call time', () => {
    // Skill exists but only in tmpDir which we never chdir into for this call
    // Using a cwd that has no .agents/skills dir
    const emptyDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-empty-')));
    try {
      process.chdir(emptyDir);
      const skills = resolveSubagentSkills(SUBAGENT_REGISTRY['researcher']);
      // tmpDir's octocode-research should NOT appear (we're in emptyDir)
      expect(skills.every(s => !s.startsWith(tmpDir))).toBe(true);
    } finally {
      process.chdir(originalCwd);
      fs.rmSync(emptyDir, { recursive: true, force: true });
    }
  });

  it('returns config.skills as-is when explicitly provided (override path)', () => {
    const explicitSkills = ['/explicit/skill-a', '/explicit/skill-b'];
    // skills override: if provided, skip all lazy resolution
    const result = resolveSubagentSkills({ skills: explicitSkills } as any);
    expect(result).toEqual(explicitSkills);
  });

  it('includes browser-agent extraSkillPaths in the resolved list when the path exists', () => {
    const ba = SUBAGENT_REGISTRY['browser-agent'];
    const extraPaths = ba.extraSkillPaths ?? [];
    const result = resolveSubagentSkills(ba);
    // Every extraSkillPath that exists on disk should appear in resolved skills
    for (const ep of extraPaths) {
      if (fs.existsSync(path.join(ep, 'SKILL.md'))) {
        expect(result).toContain(ep);
      }
    }
    expect(Array.isArray(result)).toBe(true);
  });
});
