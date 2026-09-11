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
  resolveSubagentSkills,
  SUBAGENT_REGISTRY,
} from '../src/subagents.js';

// ─── canonical discovery reuse ────────────────────────────────────────────────

describe('canonical subagent skill discovery', () => {
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

  it('keeps unrelated enabled skills out of role defaults and allows explicit role selection', () => {
    const skillDir = path.join(tmpDir, '.agents', 'skills', 'future-octocode-workflow');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '---\nname: future-octocode-workflow\ndescription: Future workflow.\n---\n');
    expect(resolveSubagentSkills(SUBAGENT_REGISTRY.researcher, tmpDir)).not.toContain(skillDir);
    expect(resolveSubagentSkills({ skillNames: ['future-octocode-workflow'] }, tmpDir)).toContain(skillDir);
  });
});

// ─── SUBAGENT_REGISTRY — no eager skills field ────────────────────────────────

describe('SUBAGENT_REGISTRY', () => {
  it('ships a read-only reviewer with no shell or file mutation tool', () => {
    const reviewer = SUBAGENT_REGISTRY['reviewer'];
    expect(reviewer.tools).toEqual(['MCPTool', 'skill', 'awareness']);
    expect(reviewer.systemPromptPath).toMatch(/reviewer[/\\]SYSTEM_PROMPT\.md$/);
  });

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

  it('every typed profile can load skills and use Awareness; mutating profiles get artifact and shell tools', () => {
    for (const [name, profile] of Object.entries(SUBAGENT_REGISTRY)) {
      expect(profile.tools).toEqual(expect.arrayContaining(['skill', 'awareness']));
      if (name !== 'reviewer') expect(profile.tools).toEqual(expect.arrayContaining(['file', 'bash']));
      expect(profile.tools).not.toContain('write');
      expect(profile.tools).not.toContain('memory');
      for (const recursiveTool of ['spawnAgent', 'spawnSubagent', 'AgentMessage']) {
        expect(profile.tools).not.toContain(recursiveTool);
      }
    }
  });

  it('preserves specialist research tools without exposing browser control to other profiles', () => {
    for (const [name, profile] of Object.entries(SUBAGENT_REGISTRY)) {
      const expected = name === 'reviewer'
        ? ['MCPTool', 'skill', 'awareness']
        : ['MCPTool', 'file', 'skill', 'awareness', 'bash'];
      if (name !== 'implementer' && name !== 'reviewer') expected.push('web');
      if (name === 'browser-agent') expected.push('chromeDebug');
      expect([...profile.tools].sort()).toEqual(expected.sort());
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
    // Create a skill AFTER the module was already loaded.
    const skillDir = path.join(tmpDir, '.agents', 'skills', 'late-installed-workflow');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '---\nname: late-installed-workflow\ndescription: Late workflow.\n---\n');

    const skills = resolveSubagentSkills({ ...SUBAGENT_REGISTRY['researcher'], skillNames: ['late-installed-workflow'] }, tmpDir);
    expect(skills).toContain(skillDir);
  });

  it('includes one usable Awareness skill for every typed profile', () => {
    const skillDir = path.join(tmpDir, '.agents', 'skills', 'octocode-awareness');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '---\nname: octocode-awareness\ndescription: Coordinate shared work.\n---\n');

    process.chdir(tmpDir);
    for (const profile of Object.values(SUBAGENT_REGISTRY)) {
      const skills = resolveSubagentSkills(profile, tmpDir).filter(s => path.basename(s) === 'octocode-awareness');
      expect(skills).toHaveLength(1);
      expect(fs.existsSync(path.join(skills[0]!, 'SKILL.md'))).toBe(true);
    }
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

  it('keeps explicit empty skills and unspecified custom defaults empty', () => {
    expect(resolveSubagentSkills({ skills: [], skillNames: ['octocode-research'] }, tmpDir)).toEqual([]);
    expect(resolveSubagentSkills({}, tmpDir)).toEqual([]);
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
