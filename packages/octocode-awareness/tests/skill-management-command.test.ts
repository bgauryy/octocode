import {
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { runSkillCheck } from '../src/skill-check-command.js';
import { runSkillInstall } from '../src/skill-install-command.js';
import { runSkillList } from '../src/skill-list-command.js';
import { runSkillRemove } from '../src/skill-remove-command.js';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const roots: string[] = [];

function tempRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-skill-manage-'));
  roots.push(root);
  return root;
}

function setup() {
  const root = tempRoot();
  const projectDir = join(root, 'project');
  const homeDir = join(root, 'home');
  const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
  mkdirSync(projectDir, { recursive: true });
  return {
    root,
    projectDir,
    homeDir,
    canonicalSkillsDir,
    skillsDir: resolve(PACKAGE_ROOT, 'skills'),
    cwd: root,
  };
}

afterEach(() => {
  for (const root of roots.splice(0))
    rmSync(root, { recursive: true, force: true });
});

describe('skill list command', () => {
  it('lists the one bundled skill and its canonical install state', () => {
    const options = setup();
    const absent = runSkillList({}, options);

    expect(absent.exitCode).toBe(0);
    expect(absent.payload).toMatchObject({
      ok: true,
      action: 'list',
      source: 'bundled',
      count: 1,
      installedCount: 0,
      skills: [
        {
          name: 'octocode-awareness',
          canonicalStatus: 'missing',
          installed: false,
        },
      ],
    });

    const installed = runSkillInstall(
      ['--platform', 'codex', '--project-dir', options.projectDir],
      options
    );
    expect(installed.exitCode).toBe(0);

    expect(runSkillList([], options).payload).toMatchObject({
      ok: true,
      installedCount: 1,
      skills: [
        {
          name: 'octocode-awareness',
          canonicalStatus: 'installed',
          installed: true,
        },
      ],
    });
  });
});

describe('skill check command', () => {
  it('checks the canonical copy alone unless a scoped platform is selected', () => {
    const options = setup();
    const missing = runSkillCheck({}, options);
    expect(missing.exitCode).toBe(1);
    expect(missing.payload).toMatchObject({
      ok: false,
      action: 'check',
      skill: { canonicalStatus: 'missing', destinations: [] },
      summary: { healthy: 0, missing: 1 },
    });

    expect(
      runSkillCheck({ platform: 'codex' }, options).payload.error
    ).toContain('--global or --project-dir');

    expect(
      runSkillInstall(
        ['--platform', 'shared', '--project-dir', options.projectDir],
        options
      ).exitCode
    ).toBe(0);

    const checked = runSkillCheck(
      {
        platform: 'shared,codex',
        project_dir: options.projectDir,
      },
      options
    );
    expect(checked.exitCode).toBe(0);
    expect(checked.payload).toMatchObject({
      ok: true,
      action: 'check',
      skill: {
        canonicalStatus: 'installed',
        destinations: [
          {
            platform: 'codex',
            scope: 'project',
            status: 'linked',
          },
        ],
      },
      summary: { healthy: 2, missing: 0, broken: 0, drifted: 0 },
    });
  });

  it('reports outdated canonical content and broken or drifted destinations', () => {
    const options = setup();
    expect(
      runSkillInstall(
        ['--platform', 'codex', '--project-dir', options.projectDir],
        options
      ).exitCode
    ).toBe(0);

    const canonical = join(
      options.canonicalSkillsDir,
      'octocode-awareness'
    );
    writeFileSync(join(canonical, 'SKILL.md'), '# stale canonical\n');
    const outdated = runSkillCheck(
      ['--platform', 'codex', '--project-dir', options.projectDir],
      options
    );
    expect(outdated.exitCode).toBe(1);
    expect(outdated.payload).toMatchObject({
      ok: false,
      skill: {
        canonicalStatus: 'outdated',
        destinations: [{ status: 'linked' }],
      },
      summary: { outdated: 1 },
    });

    const destination = join(
      options.projectDir,
      '.agents/skills/octocode-awareness'
    );
    rmSync(destination, { recursive: true, force: true });
    mkdirSync(destination, { recursive: true });
    writeFileSync(join(destination, 'SKILL.md'), '# unmanaged\n');

    const drifted = runSkillCheck(
      ['--platform', 'codex', '--project-dir', options.projectDir],
      options
    );
    expect(drifted.payload).toMatchObject({
      ok: false,
      skill: { destinations: [{ status: 'drifted' }] },
      summary: { drifted: 1 },
    });
  });

  it('rejects ambiguous scopes and invalid platforms', () => {
    const options = setup();
    expect(
      runSkillCheck(
        ['--platform', 'codex', '--global', '--project-dir', options.projectDir],
        options
      ).payload.error
    ).toContain('either --global or --project-dir');
    expect(
      runSkillCheck(['--platform', 'unknown', '--global'], options).payload
        .error
    ).toContain('Unknown platform');
  });
});

describe('skill remove command', () => {
  it('previews by default and removes only the selected platform after confirmation', () => {
    const options = setup();
    expect(
      runSkillInstall(
        ['--platform', 'codex', '--project-dir', options.projectDir],
        options
      ).exitCode
    ).toBe(0);
    const canonical = join(
      options.canonicalSkillsDir,
      'octocode-awareness'
    );
    const destination = join(
      options.projectDir,
      '.agents/skills/octocode-awareness'
    );

    const preview = runSkillRemove(
      ['--platform', 'shared', '--project-dir', options.projectDir],
      options
    );
    expect(preview.exitCode).toBe(0);
    expect(preview.payload).toMatchObject({
      ok: true,
      action: 'dry-run',
      dryRun: true,
      canonicalRemoved: false,
      targets: [
        {
          platform: 'codex',
          scope: 'project',
          path: destination,
          status: 'would-remove',
        },
      ],
      summary: { wouldRemove: 1, removed: 0, failed: 0 },
    });
    expect(existsSync(destination)).toBe(true);
    expect(existsSync(canonical)).toBe(true);

    const removed = runSkillRemove(
      {
        platform: 'codex',
        project_dir: options.projectDir,
        confirm: true,
      },
      options
    );
    expect(removed.exitCode).toBe(0);
    expect(removed.payload).toMatchObject({
      ok: true,
      action: 'remove',
      dryRun: false,
      canonicalRemoved: false,
      targets: [{ status: 'removed' }],
      summary: { removed: 1, failed: 0 },
    });
    expect(existsSync(destination)).toBe(false);
    expect(existsSync(canonical)).toBe(true);
  });

  it('removes the canonical copy only after explicit selection and confirmation', () => {
    const options = setup();
    expect(
      runSkillInstall(
        ['--platform', 'codex', '--project-dir', options.projectDir],
        options
      ).exitCode
    ).toBe(0);
    const canonical = join(
      options.canonicalSkillsDir,
      'octocode-awareness'
    );
    const destination = join(
      options.projectDir,
      '.agents/skills/octocode-awareness'
    );

    expect(runSkillRemove(['--canonical'], options).payload).toMatchObject({
      ok: true,
      action: 'dry-run',
      canonicalSelected: true,
      canonicalRemoved: false,
      targets: [{ target: 'canonical', status: 'would-remove' }],
    });
    expect(existsSync(canonical)).toBe(true);

    expect(
      runSkillRemove(['--canonical', '--confirm'], options).payload
    ).toMatchObject({
      ok: true,
      action: 'remove',
      canonicalSelected: true,
      canonicalRemoved: true,
      targets: [{ target: 'canonical', status: 'removed' }],
    });
    expect(existsSync(canonical)).toBe(false);
    expect(lstatSync(destination).isSymbolicLink()).toBe(true);
    expect(existsSync(destination)).toBe(false);
  });

  it('rejects ambiguous destructive requests', () => {
    const options = setup();
    expect(runSkillRemove([], options).payload.error).toContain(
      '--platform or --canonical'
    );
    expect(
      runSkillRemove(['--platform', 'codex'], options).payload.error
    ).toContain('--global or --project-dir');
    expect(
      runSkillRemove(
        ['--canonical', '--platform', 'codex', '--global'],
        options
      ).payload.error
    ).toContain('either --canonical or --platform');
    expect(
      runSkillRemove(['--canonical', '--confirm', '--dry-run'], options).payload
        .error
    ).toContain('either --confirm or --dry-run');
  });
});

it('keeps the installed skill byte-for-byte available through its platform link', () => {
  const options = setup();
  runSkillInstall(
    ['--platform', 'codex', '--project-dir', options.projectDir],
    options
  );
  expect(
    readFileSync(
      join(options.projectDir, '.agents/skills/octocode-awareness/SKILL.md'),
      'utf8'
    )
  ).toBe(
    readFileSync(
      join(options.skillsDir, 'octocode-awareness/SKILL.md'),
      'utf8'
    )
  );
});
