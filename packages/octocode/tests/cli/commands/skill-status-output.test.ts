import fs from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const sandbox = vi.hoisted(() => ({ root: '' }));

vi.mock('../../../src/cli/commands/skills/home.js', () => ({
  getSkillsHome: () => path.join(sandbox.root, 'home', 'skills'),
}));

vi.mock(
  '../../../src/cli/commands/skills/platforms.js',
  async importOriginal => ({
    ...(await importOriginal<object>()),
    getPlatformSkillsDir: (platform: string) =>
      path.join(sandbox.root, 'platforms', platform),
  })
);

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  dim: (s: string) => s,
  bold: (s: string) => s,
}));

import { runList } from '../../../src/cli/commands/skills/commands/list.js';
import { runCheck } from '../../../src/cli/commands/skills/commands/check.js';
import { getSkill } from '../../../src/cli/commands/skills/registry.js';

const SKILL = 'octocode-research';

function output(): string {
  return vi
    .mocked(console.log)
    .mock.calls.map(call => call.join(' '))
    .join('\n');
}

describe('skill status output surfaces stale installs', () => {
  beforeEach(() => {
    sandbox.root = fs.mkdtempSync(
      path.join(tmpdir(), 'octocode-skill-status-')
    );
    const bundled = getSkill(SKILL);
    if (!bundled)
      throw new Error(`${SKILL} is not bundled; run the build first`);
    const installed = path.join(sandbox.root, 'home', 'skills', SKILL);
    fs.cpSync(bundled.dir, installed, { recursive: true });
    fs.writeFileSync(path.join(installed, 'SKILL.md'), '# drifted\n');
    process.exitCode = undefined;
    // Belt and braces: nothing under test may reach the developer's real home.
    vi.stubEnv('HOME', path.join(sandbox.root, 'home'));
    vi.stubEnv('OCTOCODE_HOME', path.join(sandbox.root, 'home'));
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllEnvs();
    process.exitCode = undefined;
    fs.rmSync(sandbox.root, { recursive: true, force: true });
  });

  it('list marks a drifted install stale and points at check --fix', () => {
    runList({ json: false });
    const text = output();
    expect(text).toMatch(/installed · 1 stale/);
    expect(text).toContain(`${SKILL} [stale]`);
    expect(text).toContain('octocode skill check --fix');
  });

  it('list JSON exposes hasStale and staleCount', () => {
    runList({ json: true });
    const parsed = JSON.parse(output()) as {
      staleCount: number;
      skills: Array<{ name: string; hasStale: boolean }>;
    };
    expect(parsed.staleCount).toBe(1);
    expect(parsed.skills.find(skill => skill.name === SKILL)?.hasStale).toBe(
      true
    );
  });

  it('check summary counts ok/stale separately and suggests --fix', () => {
    runCheck({
      names: [SKILL],
      platform: null,
      workspace: false,
      fix: false,
      dryRun: false,
      noEnv: true,
      json: false,
    });
    const text = output();
    expect(text).toContain(`${SKILL}: stale`);
    expect(text).toContain('0/1 ok; 1 stale; 0 broken; 0 not installed');
    expect(text).toContain('Repair with octocode skill check --fix');
    expect(process.exitCode).toBe(1);
  });

  it('check --fix refreshes only existing locations, never new platforms or workspace', () => {
    runCheck({
      names: [SKILL],
      platform: null,
      workspace: false,
      fix: true,
      dryRun: false,
      noEnv: true,
      json: false,
    });
    expect(output()).toContain(`${SKILL}: ok`);
    expect(fs.existsSync(path.join(sandbox.root, 'platforms'))).toBe(false);
    expect(
      fs.existsSync(path.join(process.cwd(), '.agents', 'skills', SKILL))
    ).toBe(false);
  });
});
