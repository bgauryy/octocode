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
    expect(text).toMatch(new RegExp(`⚠ ${SKILL} +stale`));
    expect(text).toContain('octocode skill check --fix');
    // Descriptions live in `skill info`, not the list.
    expect(text).not.toContain(getSkill(SKILL)?.description ?? '∅');
  });

  it('list JSON carries the same status as check', () => {
    runList({ json: true });
    const parsed = JSON.parse(output()) as {
      installedCount: number;
      skills: Array<{ name: string; status: string }>;
    };
    expect(parsed.skills.find(skill => skill.name === SKILL)?.status).toBe(
      'stale'
    );
    expect(parsed.installedCount).toBeGreaterThanOrEqual(1);
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
    expect(text).toContain(`${SKILL} stale`);
    expect(text).toContain(
      '0 ok, 0 not installed, 1 stale, 0 broken, 0 retired'
    );
    expect(text).toContain('Repair: octocode skill check --fix');
    expect(process.exitCode).toBe(1);
  });

  it('a link the user made to a checkout outside the skills home is theirs: ok, never stale', () => {
    const checkout = path.join(sandbox.root, 'checkout', SKILL);
    fs.cpSync(path.join(sandbox.root, 'home', 'skills', SKILL), checkout, {
      recursive: true,
    });
    fs.rmSync(path.join(sandbox.root, 'home', 'skills', SKILL), {
      recursive: true,
      force: true,
    });
    const link = path.join(sandbox.root, 'platforms', 'claude', SKILL);
    fs.mkdirSync(path.dirname(link), { recursive: true });
    fs.symlinkSync(checkout, link);
    runCheck({
      names: [SKILL],
      platform: null,
      workspace: false,
      fix: false,
      dryRun: false,
      noEnv: true,
      json: false,
    });
    expect(output()).toContain(`${SKILL} ok`);
    expect(process.exitCode).toBeUndefined();
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
    expect(output()).toContain(`${SKILL} ok`);
    expect(fs.existsSync(path.join(sandbox.root, 'platforms'))).toBe(false);
    expect(
      fs.existsSync(path.join(process.cwd(), '.agents', 'skills', SKILL))
    ).toBe(false);
  });
});
