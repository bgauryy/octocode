/**
 * `octocode-skills check [<name>...]`
 *
 * Verify skill installations AND env param readiness.
 *
 * Checks:
 *   - Canonical home: ~/.octocode/skills/<name>/
 *   - Platform symlinks, workspace symlink
 *   - Broken symlinks
 *   - Required / recommended env vars
 *
 * Flags:
 *   --platform   Only check specific platforms
 *   --workspace  Also check <cwd>/.agents/skills
 *   --fix        Re-install missing/broken locations
 *   --no-env     Skip env param checks
 *   --json       Machine-readable output
 */

import path from 'node:path';
import { listSkills, getSkill } from '../registry.js';
import {
  checkSkill,
  checkSkills,
  isInstalledAtHome,
  linkedPlatforms,
  hasBroken,
  overallStatus,
  SCAN_PLATFORMS,
  type CheckedLocation,
  type SkillCheckResult,
} from '../checker.js';
import {
  getSkillEnvStatus,
  getSkillsEnvStatus,
  missingHint,
  groupLabel,
  isGroupSatisfied,
  type SkillEnvStatus,
} from '../env-params.js';
import { parsePlatforms, type Platform } from '../platforms.js';
import { installSkill } from '../installer.js';
import { getSkillsHome } from '../home.js';
import { bold, dim, green, yellow, red, cyan } from '../utils/colors.js';
import { Spinner } from '../utils/spinner.js';
import { shortPath as short } from '../utils/paths.js';

// ─── Options ──────────────────────────────────────────────────────────────────

export interface CheckOptions {
  names: string[];
  platform: string | null;
  workspace: boolean;
  fix: boolean;
  noEnv: boolean;
  json: boolean;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function locationIcon(status: CheckedLocation['status']): string {
  switch (status) {
    case 'installed': return green('✓');
    case 'linked':    return green('→');
    case 'broken':    return red('✗');
    case 'missing':   return dim('·');
  }
}

function locationLabel(loc: CheckedLocation): string {
  const icon = locationIcon(loc.status);
  const p = dim(short(loc.path));

  switch (loc.status) {
    case 'installed':
      return `${icon}  ${loc.label.padEnd(14)} ${p}  ${dim('(real copy)')}`;
    case 'linked':
      return `${icon}  ${loc.label.padEnd(14)} ${p}  ${dim('→')} ${dim(short(loc.linkTarget ?? ''))}`;
    case 'broken':
      return `${icon}  ${loc.label.padEnd(14)} ${p}  ${red('broken symlink')} → ${dim(short(loc.linkTarget ?? ''))}`;
    case 'missing':
      return `${icon}  ${loc.label.padEnd(14)} ${dim('not installed')}`;
  }
}

function envIcon(readiness: SkillEnvStatus['readiness']): string {
  switch (readiness) {
    case 'ok':
    case 'ready':        return green('✓');
    case 'partial':      return yellow('⚠');
    case 'needs-config': return red('✗');
  }
}

// ─── JSON shape ───────────────────────────────────────────────────────────────

interface JsonCheckResult {
  success: boolean;
  skills: Array<{
    name: string;
    installStatus: 'ok' | 'broken' | 'not-installed';
    home: { path: string; status: string; linkTarget?: string };
    platforms: Array<{ label: string; path: string; status: string; linkTarget?: string }>;
    workspace: { path: string; status: string; linkTarget?: string };
    env: {
      readiness: string;
      params: Array<{
        key: string;
        status: string;
        required: string;
        group?: string;
        description: string;
        link?: string;
        groupSatisfied?: boolean;
      }>;
      hint: string;
    };
  }>;
  summary: {
    install: { ok: number; broken: number; notInstalled: number; total: number };
    env: { ready: number; partial: number; needsConfig: number; noParamsNeeded: number };
  };
}

// ─── Fix ─────────────────────────────────────────────────────────────────────

function fixSkill(result: SkillCheckResult, platforms: Platform[]): void {
  const skill = getSkill(result.skillName);
  if (!skill) {
    console.log(`  ${red('✗')}  Cannot fix ${result.skillName}: not found in bundled skills`);
    return;
  }

  installSkill({
    sourcePath: skill.dir,
    skillName: skill.folder,
    platforms:
      result.home.status === 'missing' || result.home.status === 'broken'
        ? platforms
        : platforms.filter((p) => {
            const loc = result.platforms.find((pl) => pl.label === p);
            return !loc || loc.status === 'missing' || loc.status === 'broken';
          }),
    workspace: result.workspace.status === 'missing' || result.workspace.status === 'broken',
    customPath: null,
    mode: 'symlink',
    force: true,
    dryRun: false,
  });
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export function runCheck(opts: CheckOptions): void {
  // Resolve skill list
  let skillNames: string[];

  if (opts.names.length > 0) {
    skillNames = opts.names;
    for (const n of skillNames) {
      if (!getSkill(n)) {
        const msg = `Skill not found: "${n}"`;
        if (opts.json) {
          console.log(JSON.stringify({ success: false, error: msg }));
        } else {
          console.error(`  ${red('✗')} ${msg}`);
          console.error(`  Run ${cyan('octocode-skills list')} to see available skills.`);
        }
        process.exitCode = 1;
        return;
      }
    }
  } else {
    skillNames = listSkills().map((s) => s.folder);
  }

  // Resolve platforms
  let platforms: Platform[] = SCAN_PLATFORMS;
  if (opts.platform) {
    const parsed = parsePlatforms(opts.platform);
    if (parsed.error) {
      if (opts.json) {
        console.log(JSON.stringify({ success: false, error: parsed.error }));
      } else {
        console.error(`  ${red('✗')} ${parsed.error}`);
      }
      process.exitCode = 1;
      return;
    }
    platforms = parsed.platforms;
  }

  // Check installations
  const spinner = opts.json ? null : new Spinner(`Checking ${skillNames.length} skill(s)…`).start();
  let results = checkSkills(skillNames, platforms);
  const envStatuses = opts.noEnv ? [] : getSkillsEnvStatus(skillNames);
  spinner?.stop();

  // Fix
  if (opts.fix && !opts.json) {
    for (const r of results) {
      if (overallStatus(r) !== 'ok') {
        console.log(`  ${cyan('→')} Fixing ${bold(r.skillName)}…`);
        fixSkill(r, platforms);
      }
    }
    results = checkSkills(skillNames, platforms);
  }

  // Summaries
  let okCount = 0, brokenCount = 0, notInstalledCount = 0;
  for (const r of results) {
    const s = overallStatus(r);
    if (s === 'ok') okCount++;
    else if (s === 'broken') brokenCount++;
    else notInstalledCount++;
  }

  let envReadyCount = 0, envPartialCount = 0, envNeedsConfigCount = 0, envNoParamsCount = 0;
  if (!opts.noEnv) {
    for (const e of envStatuses) {
      if (e.readiness === 'ok') envNoParamsCount++;
      else if (e.readiness === 'ready') envReadyCount++;
      else if (e.readiness === 'partial') envPartialCount++;
      else envNeedsConfigCount++;
    }
  }

  const installOk = brokenCount === 0;
  const envOk = opts.noEnv || envNeedsConfigCount === 0;
  const allOk = installOk && envOk;

  // ── JSON ─────────────────────────────────────────────────────────────────

  if (opts.json) {
    const out: JsonCheckResult = {
      success: allOk,
      skills: results.map((r, i) => {
        const env = envStatuses[i] ?? getSkillEnvStatus(r.skillName);
        return {
          name: r.skillName,
          installStatus: overallStatus(r),
          home: {
            path: r.home.path,
            status: r.home.status,
            ...(r.home.linkTarget ? { linkTarget: r.home.linkTarget } : {}),
          },
          platforms: r.platforms.map((p) => ({
            label: p.label,
            path: p.path,
            status: p.status,
            ...(p.linkTarget ? { linkTarget: p.linkTarget } : {}),
          })),
          workspace: {
            path: r.workspace.path,
            status: r.workspace.status,
            ...(r.workspace.linkTarget ? { linkTarget: r.workspace.linkTarget } : {}),
          },
          env: {
            readiness: env.readiness,
            params: env.params.map((ps) => ({
              key: ps.param.key,
              status: ps.status,
              required: ps.param.required,
              description: ps.param.description,
              ...(ps.param.group ? { group: ps.param.group, groupSatisfied: isGroupSatisfied(ps, env.params) } : {}),
              ...(ps.param.link ? { link: ps.param.link } : {}),
            })),
            hint: missingHint(env),
          },
        };
      }),
      summary: {
        install: { ok: okCount, broken: brokenCount, notInstalled: notInstalledCount, total: results.length },
        env: opts.noEnv
          ? { ready: 0, partial: 0, needsConfig: 0, noParamsNeeded: skillNames.length }
          : { ready: envReadyCount, partial: envPartialCount, needsConfig: envNeedsConfigCount, noParamsNeeded: envNoParamsCount },
      },
    };
    console.log(JSON.stringify(out, null, 2));
    if (!allOk) process.exitCode = 1;
    return;
  }

  // ── Human ────────────────────────────────────────────────────────────────

  const skillsHome = short(getSkillsHome());
  console.log();
  console.log(`  ${bold('Skill check')}  ${dim(`· home: ${skillsHome}`)}`);
  console.log();

  for (let i = 0; i < results.length; i++) {
    const r = results[i]!;
    const env = envStatuses[i] ?? getSkillEnvStatus(r.skillName);
    const st = overallStatus(r);

    const installBadge =
      st === 'ok' ? green('✓') : st === 'broken' ? red('✗') : yellow('–');

    const links = linkedPlatforms(r);
    const hasHome = isInstalledAtHome(r);
    const installSummary = hasHome
      ? dim(`installed${links.length ? ` · linked: ${links.join(', ')}` : ''}`)
      : st === 'broken'
        ? red('broken symlink(s)')
        : dim('not installed');

    const envSummary = opts.noEnv
      ? ''
      : `  ${envIcon(env.readiness)} env: ${
          env.readiness === 'ok' || env.readiness === 'ready'
            ? dim(env.readiness === 'ok' ? 'none needed' : 'ready')
            : yellow(missingHint(env) || env.readiness)
        }`;

    console.log(`  ${installBadge}  ${bold(r.skillName)}  ${installSummary}${envSummary}`);

    // Install locations
    console.log(`       ${locationLabel(r.home)}`);

    const platformsToShow = opts.platform
      ? r.platforms
      : r.platforms.filter((p) => p.status !== 'missing');

    for (const p of platformsToShow) {
      console.log(`       ${locationLabel(p)}`);
    }

    if (opts.workspace || r.workspace.status !== 'missing') {
      console.log(`       ${locationLabel(r.workspace)}`);
    }

    // Env params section (skip if none needed)
    if (!opts.noEnv && env.readiness !== 'ok') {
      const shownGroups = new Set<string>();
      for (const ps of env.params) {
        const { group } = ps.param;
        const satisfied = isGroupSatisfied(ps, env.params);

        if (group) {
          if (shownGroups.has(group)) continue;
          shownGroups.add(group);
          // Show group as a single row
          const anySet = env.params.some((p) => p.param.group === group && p.status === 'set');
          const icon = anySet ? green('✓') : ps.param.required === 'required' ? red('✗') : yellow('⚠');
          const keys = env.params
            .filter((p) => p.param.group === group)
            .map((p) => (p.status === 'set' ? green(p.param.key) : dim(p.param.key)))
            .join(dim(' | '));
          const label = anySet ? dim(groupLabel(group)) : yellow(groupLabel(group));
          console.log(`       ${icon}  ${'env'.padEnd(14)} ${label}  ${dim('→')}  ${keys}`);
        } else {
          const icon = ps.status === 'set'
            ? green('✓')
            : ps.param.required === 'required' ? red('✗') : yellow('⚠');
          const keyStr = ps.status === 'set' ? green(ps.param.key) : yellow(ps.param.key);
          const statusStr = ps.status === 'set' ? dim('set') : dim(`${ps.param.required} — not set`);
          console.log(`       ${icon}  ${'env'.padEnd(14)} ${keyStr}  ${statusStr}`);
        }
      }
    }

    console.log();
  }

  // Summary bar
  console.log(`  ${dim('─'.repeat(60))}`);

  const installParts = [
    `${green(String(okCount))} ok`,
    ...(notInstalledCount ? [`${yellow(String(notInstalledCount))} not installed`] : []),
    ...(brokenCount ? [`${red(String(brokenCount))} broken`] : []),
  ];
  console.log(`  ${dim('Install:')} ${installParts.join('  ·  ')}`);

  if (!opts.noEnv) {
    const envParts = [
      ...(envNoParamsCount + envReadyCount > 0 ? [`${green(String(envNoParamsCount + envReadyCount))} ready`] : []),
      ...(envPartialCount ? [`${yellow(String(envPartialCount))} partial`] : []),
      ...(envNeedsConfigCount ? [`${red(String(envNeedsConfigCount))} needs config`] : []),
    ];
    if (envParts.length > 0) {
      console.log(`  ${dim('Env:')}     ${envParts.join('  ·  ')}`);
    }
  }

  console.log();

  // Actionable hints
  if (notInstalledCount > 0) {
    console.log(`  ${dim('Install missing:')}   ${cyan('octocode-skills install --all')}`);
  }
  if (brokenCount > 0) {
    console.log(`  ${dim('Fix broken links:')}  ${cyan('octocode-skills check --fix')}`);
  }

  if (!opts.noEnv && (envNeedsConfigCount > 0 || envPartialCount > 0)) {
    console.log(`  ${dim('Env params missing — add to')} ${dim('~/.octocode/.env')}${dim(':')}`);
    console.log();

    // Collect all unsatisfied params (deduplicate groups)
    const shownGroups = new Set<string>();
    const shownKeys = new Set<string>();

    for (const env of envStatuses) {
      if (env.readiness === 'ok' || env.readiness === 'ready') continue;

      for (const ps of env.params) {
        if (ps.status === 'set') continue;
        if (isGroupSatisfied(ps, env.params)) continue;

        const { group, key, required, link } = ps.param;

        if (group) {
          if (shownGroups.has(group)) continue;
          shownGroups.add(group);
          const groupKeys = env.params.filter((p) => p.param.group === group);
          console.log(`  ${dim(groupLabel(group))}  ${dim(`[${required}]`)}`);
          for (const gp of groupKeys) {
            const linkStr = gp.param.link ? `  ${dim(gp.param.link)}` : '';
            console.log(`    ${gp.param.key}=...${linkStr}`);
          }
          console.log();
        } else {
          if (shownKeys.has(key)) continue;
          shownKeys.add(key);
          const linkStr = link ? `  ${dim(link)}` : '';
          console.log(`  ${key}=...  ${dim(`[${required}]`)}${linkStr}`);
        }
      }
    }
  }

  if (!allOk) process.exitCode = 1;
}
