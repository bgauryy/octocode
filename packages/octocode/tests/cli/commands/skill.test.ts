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
import { findCommandSpec } from '../../../src/cli/commands/specs.js';

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

  it('declares only canonical bundled-skill options', () => {
    const optNames = (skillCommand.options ?? []).map(o => o.name);
    const required = [
      'add',
      'platform',
      'all',
      'mode',
      'force',
      'upgrade',
      'global',
      'project-dir',
      'workspace',
      'path',
      'dry-run',
      'json',
      'fix',
      'no-env',
    ];
    for (const opt of required) {
      expect(optNames, `missing option --${opt}`).toContain(opt);
    }
    expect(skillCommand.options?.find(o => o.name === 'add')?.hasValue).toBe(
      true
    );
    for (const removed of [
      'name',
      'list',
      'target',
      'keep',
      'update',
      'verbose',
      'branch',
      'install-all',
      'all-skills',
      'repo',
    ]) {
      expect(optNames).not.toContain(removed);
    }
  });

  it('documents the same install flags in generated command help', () => {
    const helpOptions = findCommandSpec('skill')?.options ?? [];
    const names = helpOptions.map(option => option.name);
    expect(names).toEqual(
      expect.arrayContaining([
        'platform',
        'mode',
        'force',
        'upgrade',
        'global',
        'project-dir',
        'dry-run',
      ])
    );
    expect(names).not.toContain('keep');
    expect(
      helpOptions.find(option => option.name === 'workspace')?.description
    ).toContain('check only');
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
      ok: boolean;
      dryRun: boolean;
      skills: Array<{ name: string; canonical: string }>;
      summary: { installed: number; failed: number };
    }>();
    expect(parsed.ok).toBe(true);
    expect(parsed.dryRun).toBe(true);
    expect(parsed.skills[0]?.name).toBe('octocode-research');
    expect(parsed.skills[0]?.canonical).toBe(
      '/mock-home/.octocode/skills/octocode-research'
    );
    expect(parsed.summary.failed).toBe(0);
  });

  it('passes the explicit upgrade contract to the shared installer', () => {
    run(['install', 'octocode-research'], {
      upgrade: true,
      'dry-run': true,
      json: true,
    });
    expect(loggedJson<{ ok: boolean; upgrade: boolean }>()).toMatchObject({
      ok: true,
      upgrade: true,
    });
  });

  it('requires exactly one explicit scope when a platform is selected', () => {
    run(['install', 'octocode-research'], {
      platform: 'pi',
      'dry-run': true,
      json: true,
    });
    expect(loggedJson<{ ok: boolean; error: string }>()).toMatchObject({
      ok: false,
      error: expect.stringContaining('--global or --project-dir'),
    });

    vi.mocked(console.log).mockClear();
    run(['install', 'octocode-research'], {
      platform: 'pi',
      global: true,
      'project-dir': '.',
      'dry-run': true,
      json: true,
    });
    expect(loggedJson<{ ok: boolean; error: string }>()).toMatchObject({
      ok: false,
      error: expect.stringContaining('--global or --project-dir'),
    });

    vi.mocked(console.log).mockClear();
    run(['install', 'octocode-research'], {
      workspace: true,
      'dry-run': true,
      json: true,
    });
    expect(loggedJson<{ ok: boolean; error: string }>()).toMatchObject({
      ok: false,
      error: expect.stringContaining('--platform codex --project-dir'),
    });

    vi.mocked(console.log).mockClear();
    run(['install', 'octocode-research'], {
      platform: 'all',
      'project-dir': '.',
      'dry-run': true,
      json: true,
    });
    const allPlatforms = loggedJson<{
      ok: boolean;
      skills: Array<{ destinations: unknown[] }>;
    }>();
    expect(allPlatforms).toMatchObject({ ok: true });
    expect(allPlatforms.skills[0]?.destinations).toHaveLength(7);
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
      run(['install'], {
        add: sourceDir,
        platform: 'claude,cursor,codex-native',
        global: true,
        force: true,
        'dry-run': true,
        json: true,
      });

      const parsed = loggedJson<{
        ok: boolean;
        skills: Array<{
          name: string;
          canonical: string;
          destinations: Array<{
            platform: string;
            scope: string;
            destination: string;
            status: string;
          }>;
        }>;
      }>();
      expect(parsed.ok).toBe(true);
      expect(parsed.skills[0]?.name).toBe('fixture-skill');
      expect(parsed.skills[0]?.canonical).toBe(
        '/mock-home/.octocode/skills/fixture-skill'
      );
      expect(parsed.skills[0]?.destinations).toEqual([
        {
          platform: 'claude',
          scope: 'global',
          destination: path.join(
            homedir(),
            '.claude',
            'skills',
            'fixture-skill'
          ),
          mode: 'symlink',
          status: 'linked',
          linkTarget: '/mock-home/.octocode/skills/fixture-skill',
        },
        {
          platform: 'cursor',
          scope: 'global',
          destination: path.join(
            homedir(),
            '.cursor',
            'skills',
            'fixture-skill'
          ),
          mode: 'symlink',
          status: 'linked',
          linkTarget: '/mock-home/.octocode/skills/fixture-skill',
        },
        {
          platform: 'codex',
          scope: 'global',
          destination: path.join(
            homedir(),
            '.agents',
            'skills',
            'fixture-skill'
          ),
          mode: 'symlink',
          status: 'linked',
          linkTarget: '/mock-home/.octocode/skills/fixture-skill',
        },
      ]);

      vi.mocked(console.log).mockClear();
      run(['install'], {
        add: sourceDir,
        platform: 'codex',
        global: true,
        force: true,
        'dry-run': true,
        json: true,
      });
      const deduped = loggedJson<{
        skills: Array<{
          destinations: Array<{
            platform: string;
            destination: string;
            status: string;
          }>;
        }>;
      }>();
      expect(deduped.skills[0]?.destinations).toEqual([
        {
          platform: 'codex',
          scope: 'global',
          destination: path.join(
            homedir(),
            '.agents',
            'skills',
            'fixture-skill'
          ),
          mode: 'symlink',
          status: 'linked',
          linkTarget: '/mock-home/.octocode/skills/fixture-skill',
        },
      ]);
    } finally {
      fs.rmSync(fixtureRoot, { recursive: true, force: true });
    }
  });

  it('normalizes desktop/native aliases to current host skill homes', () => {
    expect(getPlatformSkillsDir('claude')).toBe(
      path.join(homedir(), '.claude', 'skills')
    );
    expect(
      parsePlatforms('claude,claude-desktop,cursor,codex,codex-native')
    ).toEqual({
      platforms: ['claude', 'cursor', 'codex'],
    });
  });

  it('normalizes shared skill-directory aliases', () => {
    expect(parsePlatforms('shared,common,agents,codex')).toEqual({
      platforms: ['codex'],
    });
  });

  it('rejects unsupported platform spellings', () => {
    for (const removed of [
      'pi-agent',
      'claude-code',
      'open-code',
      'github-copilot',
      'vscode-copilot',
      'gemini-cli',
      'agent',
    ]) {
      expect(parsePlatforms(removed).error).toContain('Unknown platform');
    }
  });

  it('rejects unknown skill names on install', () => {
    run(['install', 'not-a-real-skill'], { json: true });
    expect(process.exitCode).toBe(EXIT.GENERAL);
    const parsed = loggedJson<{ ok: boolean; error: string }>();
    expect(parsed.ok).toBe(false);
    expect(parsed.error).toContain('not-a-real-skill');
  });

  it('checks a bundled skill without requiring environment variables', () => {
    run(['check', 'octocode-research'], { 'no-env': true, json: true });
    const parsed = loggedJson<{
      success: boolean;
      skills: Array<{ name: string; installStatus: string }>;
      summary: { install: { total: number }; env: { needsConfig: number } };
    }>();
    expect(parsed.success).toBe(true);
    expect(parsed.skills[0]?.name).toBe('octocode-research');
    expect(parsed.summary.install.total).toBe(1);
    expect(parsed.summary.env.needsConfig).toBe(0);
  });

  it('rejects unknown skill names on check', () => {
    run(['check', 'not-a-real-skill'], { json: true });
    expect(process.exitCode).toBe(EXIT.GENERAL);
    const parsed = loggedJson<{ success: boolean; error: string }>();
    expect(parsed.success).toBe(false);
    expect(parsed.error).toContain('not-a-real-skill');
  });

  it('requires a target for remove', () => {
    run(['remove'], { json: true });
    expect(process.exitCode).toBe(EXIT.GENERAL);
    expect(loggedJson<{ success: boolean }>().success).toBe(false);
  });

  it('dry-runs remove without deleting installed locations', () => {
    run(['remove', 'octocode-research'], { 'dry-run': true, json: true });
    const parsed = loggedJson<{
      success: boolean;
      skills: Array<{ name: string; nothingFound: boolean }>;
      summary: { removed: number; failed: number };
    }>();
    expect(parsed.success).toBe(true);
    expect(parsed.skills[0]?.name).toBe('octocode-research');
    expect(typeof parsed.skills[0]?.nothingFound).toBe('boolean');
    expect(parsed.summary.failed).toBe(0);
  });
});
