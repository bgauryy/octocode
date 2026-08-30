import fs from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import path from 'node:path';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('@octocodeai/config', () => ({
  getOctocodeHome: () => '/mock-home/.octocode',
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  dim: (s: string) => s,
  bold: (s: string) => s,
}));

import { skillCommand } from '../../../src/cli/commands/skill.js';
import {
  getPlatformSkillsDir,
  parsePlatforms,
} from '../../../src/cli/commands/skills/platforms.js';
import type { ParsedArgs } from '../../../src/cli/types.js';
import { EXIT } from '../../../src/cli/exit-codes.js';

function run(
  args: string[] = [],
  options: Record<string, string | boolean> = {}
) {
  const parsed: ParsedArgs = { command: 'skill', args, options };
  return skillCommand.handler(parsed);
}

function loggedJson<T>(): T {
  const logArg = (console.log as ReturnType<typeof vi.spyOn>).mock
    .calls[0][0] as string;
  return JSON.parse(logArg) as T;
}

describe('skill command', () => {
  beforeEach(() => {
    process.exitCode = undefined;
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    process.exitCode = undefined;
  });

  it('has name "skill"', () => {
    expect(skillCommand.name).toBe('skill');
  });

  it('declares legacy and bundled-skill options', () => {
    const optNames = (skillCommand.options ?? []).map(o => o.name);
    const required = [
      'add',
      'name',
      'list',
      'platform',
      'target',
      'all',
      'mode',
      'force',
      'update',
      'dry-run',
      'verbose',
      'branch',
      'json',
      'install-all',
      'all-skills',
      'keep',
      'workspace',
      'repo',
      'path',
      'fix',
      'no-env',
    ];
    for (const opt of required) {
      expect(optNames, `missing option --${opt}`).toContain(opt);
    }
    expect(skillCommand.options?.find(o => o.name === 'add')?.hasValue).toBe(
      true
    );
  });

  it('prints bundled skill help when no subcommand is provided', () => {
    run([], {});
    expect(process.exitCode).toBeUndefined();
    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining('octocode skill')
    );
  });

  it('lists bundled skills as JSON with install/env status', () => {
    run(['list'], { json: true });
    const parsed = loggedJson<{
      success: boolean;
      count: number;
      skills: Array<{ name: string; env: unknown }>;
    }>();
    expect(parsed.success).toBe(true);
    expect(parsed.count).toBeGreaterThan(0);
    expect(parsed.skills.some(s => s.name === 'octocode-research')).toBe(true);
    expect(parsed.skills[0]?.env).toBeDefined();
  });

  it('keeps --list as an alias for skill list', () => {
    run([], { list: true, json: true });
    const parsed = loggedJson<{ success: boolean; count: number }>();
    expect(parsed.success).toBe(true);
    expect(parsed.count).toBeGreaterThan(0);
  });

  it('shows skill info as JSON', () => {
    run(['info', 'octocode-research'], { json: true });
    const parsed = loggedJson<{
      success: boolean;
      skill: { name: string; skillMd: string };
    }>();
    expect(parsed.success).toBe(true);
    expect(parsed.skill.name).toBe('octocode-research');
    expect(parsed.skill.skillMd).toContain('name: octocode-research');
  });

  it('exits USAGE for info without a skill name', () => {
    run(['info'], {});
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  it('dry-runs install from bundled skills without writing', () => {
    run(['install', 'octocode-research'], { 'dry-run': true, json: true });
    const parsed = loggedJson<{
      success: boolean;
      dryRun: boolean;
      skills: Array<{ name: string; home: string | null }>;
      summary: { installed: number; failed: number };
    }>();
    expect(parsed.success).toBe(true);
    expect(parsed.dryRun).toBe(true);
    expect(parsed.skills[0]?.name).toBe('octocode-research');
    expect(parsed.skills[0]?.home).toBe(
      '/mock-home/.octocode/skills/octocode-research'
    );
    expect(parsed.summary.failed).toBe(0);
  });

  it('supports --name as install alias', () => {
    run([], { name: 'octocode-research', 'dry-run': true, json: true });
    const parsed = loggedJson<{
      success: boolean;
      skills: Array<{ name: string }>;
    }>();
    expect(parsed.success).toBe(true);
    expect(parsed.skills[0]?.name).toBe('octocode-research');
  });

  it('dry-runs adding a local skill through canonical home and vendor links', () => {
    const fixtureRoot = fs.mkdtempSync(
      path.join(tmpdir(), 'octocode-skill-add-')
    );
    const sourceDir = path.join(fixtureRoot, 'fixture-skill');
    fs.mkdirSync(sourceDir);
    fs.writeFileSync(
      path.join(sourceDir, 'SKILL.md'),
      [
        '---',
        'name: fixture-skill',
        'description: "Fixture skill for CLI installation tests."',
        '---',
        '# Fixture',
      ].join('\n')
    );

    try {
      run([], {
        add: sourceDir,
        platform: 'claude,cursor,codex-native',
        'dry-run': true,
        json: true,
      });

      const parsed = loggedJson<{
        success: boolean;
        skills: Array<{
          name: string;
          home: string | null;
          links: Array<{ target: string; destPath: string }>;
        }>;
      }>();
      expect(parsed.success).toBe(true);
      expect(parsed.skills[0]?.name).toBe('fixture-skill');
      expect(parsed.skills[0]?.home).toBe(
        '/mock-home/.octocode/skills/fixture-skill'
      );
      expect(parsed.skills[0]?.links).toEqual([
        {
          target: 'agents',
          destPath: path.join(homedir(), '.agents', 'skills', 'fixture-skill'),
          status: 'linked',
        },
        {
          target: 'claude',
          destPath: path.join(homedir(), '.claude', 'skills', 'fixture-skill'),
          status: 'linked',
        },
        {
          target: 'cursor',
          destPath: path.join(homedir(), '.cursor', 'skills', 'fixture-skill'),
          status: 'linked',
        },
        {
          target: 'codex-native',
          destPath: path.join(homedir(), '.codex', 'skills', 'fixture-skill'),
          status: 'linked',
        },
      ]);

      vi.mocked(console.log).mockClear();
      run([], {
        add: sourceDir,
        platform: 'codex',
        'dry-run': true,
        json: true,
      });
      const deduped = loggedJson<{
        skills: Array<{
          links: Array<{ target: string; destPath: string; status: string }>;
        }>;
      }>();
      expect(deduped.skills[0]?.links).toEqual([
        {
          target: 'codex',
          destPath: path.join(homedir(), '.agents', 'skills', 'fixture-skill'),
          status: 'linked',
        },
      ]);
    } finally {
      fs.rmSync(fixtureRoot, { recursive: true, force: true });
    }
  });

  it('requires a source when adding a standalone skill', () => {
    run([], { add: true, json: true });
    const parsed = loggedJson<{ success: boolean; error: string }>();
    expect(parsed.success).toBe(false);
    expect(parsed.error).toContain('--add requires <source>');
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  it('maps Claude Code and native Codex to distinct global skill homes', () => {
    expect(getPlatformSkillsDir('claude')).toBe(
      path.join(homedir(), '.claude', 'skills')
    );
    expect(getPlatformSkillsDir('claude-desktop')).toBe(
      path.join(homedir(), '.claude-desktop', 'skills')
    );
    expect(getPlatformSkillsDir('codex-native')).toBe(
      path.join(homedir(), '.codex', 'skills')
    );
    expect(parsePlatforms('claude,cursor,codex,codex-native')).toEqual({
      platforms: ['claude', 'cursor', 'codex', 'codex-native'],
    });
  });

  it('rejects unknown skill names on install', () => {
    run(['install', 'not-a-real-skill'], { json: true });
    expect(process.exitCode).toBe(EXIT.GENERAL);
    const parsed = loggedJson<{ success: boolean; error: string }>();
    expect(parsed.success).toBe(false);
    expect(parsed.error).toContain('not-a-real-skill');
  });
});
