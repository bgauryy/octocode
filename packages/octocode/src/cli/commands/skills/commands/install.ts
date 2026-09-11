/** `octocode skill install [<name>...] [options]` */

import { existsSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  installBundledSkills,
  type InstallBundledSkillsResult,
  type SkillInstallTarget,
} from '@octocodeai/octocode-skill-installer';
import {
  listSkills,
  getSkill,
  getSkillFromPath,
  type SkillInfo,
} from '../registry.js';
import { parsePlatforms } from '../platforms.js';
import type { InstallMode } from '../installer.js';
import { getSkillsEnvStatus } from '../env-params.js';
import { bold, dim, c } from '../../../../utils/colors.js';
import { shortPath } from '../utils/paths.js';

export interface InstallOptions {
  all: boolean;
  /** Local standalone skill source used by `skill install --add <source>`. */
  sourcePath: string | null;
  platform: string | null;
  /** Legacy check-only flag; install rejects it with the canonical replacement. */
  workspace: boolean;
  global: boolean;
  projectDir: string | null;
  customPath: string | null;
  mode: InstallMode;
  /** Replace existing canonical copies or destinations that differ. */
  force: boolean;
  /** Refresh changed bundled content without replacing arbitrary destination drift. */
  upgrade: boolean;
  dryRun: boolean;
  json: boolean;
}

function fail(message: string, json: boolean): void {
  if (json) console.log(JSON.stringify({ ok: false, error: message }));
  else console.error(`\n  ${c('red', '✗')}  ${message}\n`);
  process.exitCode = 1;
}

function resolveSkills(
  skillNames: string[],
  opts: InstallOptions
): SkillInfo[] | null {
  if (opts.sourcePath) {
    if (opts.all || skillNames.length > 1) {
      fail(
        opts.all
          ? '--add cannot be combined with --all.'
          : '--add accepts at most one name override.',
        opts.json
      );
      return null;
    }
    const resolved = getSkillFromPath(opts.sourcePath, skillNames[0]);
    if (!resolved.skill) {
      fail(resolved.error ?? 'Unable to load local skill.', opts.json);
      return null;
    }
    return [resolved.skill];
  }

  if (opts.all) {
    const skills = listSkills();
    if (skills.length === 0) {
      fail('No bundled skills found.', opts.json);
      return null;
    }
    return skills;
  }

  if (skillNames.length === 0) {
    fail('Specify a skill name or use --all.', opts.json);
    return null;
  }

  const skills: SkillInfo[] = [];
  const missing: string[] = [];
  for (const name of skillNames) {
    const skill = getSkill(name);
    if (skill) skills.push(skill);
    else missing.push(name);
  }
  if (missing.length > 0) {
    fail(
      `Skill(s) not found: ${missing.map(name => `"${name}"`).join(', ')}`,
      opts.json
    );
    return null;
  }
  return skills;
}

function resolveTargets(opts: InstallOptions): SkillInstallTarget[] | null {
  if (!opts.platform) {
    if (opts.global || opts.projectDir) {
      fail('--global and --project-dir require --platform.', opts.json);
      return null;
    }
    return [];
  }
  if (opts.global === Boolean(opts.projectDir)) {
    fail(
      'Choose exactly one scope for --platform: --global or --project-dir <dir>.',
      opts.json
    );
    return null;
  }

  const parsed = parsePlatforms(opts.platform);
  if (parsed.error) {
    fail(parsed.error, opts.json);
    return null;
  }

  if (opts.projectDir) {
    const projectDir = resolve(opts.projectDir);
    if (!existsSync(projectDir) || !statSync(projectDir).isDirectory()) {
      fail(`Project directory does not exist: ${projectDir}`, opts.json);
      return null;
    }
    return parsed.platforms.map(platform => ({
      platform,
      scope: 'project',
      projectDir,
    }));
  }
  return parsed.platforms.map(platform => ({ platform, scope: 'global' }));
}

function statusIcon(status: string): string {
  if (
    status === 'installed' ||
    status === 'upgraded' ||
    status === 'linked' ||
    status === 'copied'
  )
    return c('green', '✓');
  if (status === 'unchanged') return c('yellow', '~');
  return c('red', '✗');
}

function renderHuman(
  result: InstallBundledSkillsResult,
  skills: SkillInfo[]
): void {
  console.log();
  console.log(
    `  ${bold(result.dryRun ? 'Skill install preview' : 'Skill installation')}`
  );
  console.log(`  ${dim(`canonical: ${shortPath(result.canonicalSkillsDir)}`)}`);
  console.log();

  for (const skill of result.skills) {
    console.log(`  ${statusIcon(skill.canonicalStatus)}  ${bold(skill.name)}`);
    const canonicalNote = skill.canonicalError
      ? c('red', `  ${skill.canonicalError}`)
      : skill.canonicalStatus === 'conflict'
        ? dim('  (differs; use --upgrade to refresh or --force to replace)')
        : '';
    console.log(
      `     ${statusIcon(skill.canonicalStatus)}  ${'canonical'.padEnd(18)} ${dim(shortPath(skill.canonical))}${canonicalNote}`
    );
    for (const destination of skill.destinations) {
      const label = `${destination.platform}:${destination.scope}`;
      const note = destination.error
        ? c('red', `  ${destination.error}`)
        : destination.status === 'conflict'
          ? dim('  (differs; use --force to replace)')
          : destination.mode === 'symlink'
            ? dim(`  → ${shortPath(destination.linkTarget ?? skill.canonical)}`)
            : dim('  (copy)');
      console.log(
        `     ${statusIcon(destination.status)}  ${label.padEnd(18)} ${dim(shortPath(destination.destination || '—'))}${note}`
      );
    }
    console.log();
  }

  const summary = result.summary;
  console.log(`  ${dim('─'.repeat(60))}`);
  console.log(
    `  ${summary.installed} materialized · ${summary.upgraded} upgraded · ${summary.linked} linked · ${summary.copied} copied · ${summary.unchanged} unchanged · ${summary.conflicts} conflicts · ${summary.failed} failed`
  );
  console.log();

  if (!result.ok) {
    console.log(
      `  ${c('red', 'Installation did not complete cleanly.')} Resolve drift, use --upgrade for bundled content, or rerun with --force.`
    );
    console.log();
    return;
  }
  if (!result.dryRun) {
    console.log(`  ${dim('Verify:')} ${c('cyan', 'octocode skill check')}`);
    const needsEnv = getSkillsEnvStatus(
      skills.map(skill => skill.folder)
    ).filter(
      status =>
        status.readiness === 'needs-config' || status.readiness === 'partial'
    );
    if (needsEnv.length > 0) {
      console.log(
        `  ${c('yellow', '⚠')} ${needsEnv.map(status => status.skillName).join(', ')} need environment configuration.`
      );
      console.log(
        `  ${dim('Configure ~/.octocode/.env, then run octocode skill check.')}`
      );
    }
    console.log();
  }
}

export function runInstall(skillNames: string[], opts: InstallOptions): void {
  if (opts.workspace) {
    fail(
      '--workspace is only valid for skill check; use --platform codex --project-dir <dir> for installation.',
      opts.json
    );
    return;
  }
  if (opts.customPath && opts.platform) {
    fail('--path cannot be combined with --platform.', opts.json);
    return;
  }
  const skills = resolveSkills(skillNames, opts);
  if (!skills) return;
  const targets = resolveTargets(opts);
  if (!targets) return;

  const result = installBundledSkills({
    skills: skills.map(skill => ({
      name: skill.folder,
      sourcePath: skill.dir,
    })),
    targets,
    canonicalSkillsDir: opts.customPath || undefined,
    mode: opts.mode,
    force: opts.force,
    upgrade: opts.upgrade,
    dryRun: opts.dryRun,
  });

  if (opts.json) console.log(JSON.stringify(result, null, 2));
  else renderHuman(result, skills);
  if (!result.ok) process.exitCode = 1;
}
