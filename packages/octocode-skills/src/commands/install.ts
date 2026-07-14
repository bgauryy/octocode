/**
 * `octocode-skills install [<name>...] [options]`
 *
 * Install one or more bundled skills.
 *
 * ─── DEFAULT BEHAVIOUR ────────────────────────────────────────────────────────
 *   Override is ON by default — installs always overwrite the existing copy
 *   so you always get the latest bundled version.
 *   Pass --keep to preserve an existing installation and skip the overwrite.
 *
 * ─── INSTALLATION MODEL ───────────────────────────────────────────────────────
 *   1. Copy skill to ~/.octocode/skills/<name>/  (canonical home, always fresh)
 *   2. Symlink from platform dirs → home          (--platform)
 *   3. Symlink from workspace    → home           (--workspace / --repo)
 *   4. Custom path               → direct copy/symlink (--path, skips home)
 */

import path from 'node:path';
import { listSkills, getSkill, type SkillInfo } from '../registry.js';
import { parsePlatforms, type Platform } from '../platforms.js';
import { installSkill, type InstallMode, type SkillInstallOutcome } from '../installer.js';
import { getSkillsHome } from '../home.js';
import { getSkillsEnvStatus, groupLabel, isGroupSatisfied } from '../env-params.js';
import { Spinner } from '../utils/spinner.js';
import { bold, dim, green, yellow, red, cyan } from '../utils/colors.js';
import { shortPath } from '../utils/paths.js';

// ─── Options ──────────────────────────────────────────────────────────────────

export interface InstallOptions {
  all: boolean;
  platform: string | null;
  workspace: boolean;
  customPath: string | null;
  mode: InstallMode;
  /**
   * Keep existing installations (skip overwrite).
   * Default: false — override is on by default.
   */
  keep: boolean;
  dryRun: boolean;
  json: boolean;
}

// ─── JSON result shape ────────────────────────────────────────────────────────

export interface InstallJsonResult {
  success: boolean;
  dryRun: boolean;
  override: boolean;
  skills: Array<{
    name: string;
    home: string | null;
    homeStatus: string;
    homeError?: string;
    links: Array<{
      target: string;
      destPath: string;
      status: string;
      error?: string;
    }>;
  }>;
  summary: { installed: number; skipped: number; failed: number };
}


function statusIcon(status: string): string {
  if (status === 'installed' || status === 'linked') return green('✓');
  if (status === 'skipped') return yellow('~');
  if (status === 'bypassed') return dim('–');
  return red('✗');
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export function runInstall(skillNames: string[], opts: InstallOptions): void {
  const force = !opts.keep; // override by default

  // ── Resolve skill list ────────────────────────────────────────────────────

  let skills: SkillInfo[];

  if (opts.all) {
    skills = listSkills();
    if (skills.length === 0) {
      if (opts.json) {
        console.log(JSON.stringify({ success: false, error: 'No bundled skills found.', skills: [], summary: { installed: 0, skipped: 0, failed: 0 } }));
      } else {
        console.log(`\n  ${red('\u2717')}  No bundled skills found.\n`);
      }
      process.exitCode = 1;
      return;
    }
  } else {
    if (skillNames.length === 0) {
      if (opts.json) {
        console.log(JSON.stringify({
          success: false,
          error: 'Specify a skill name or use --all.',
          skills: [],
          summary: { installed: 0, skipped: 0, failed: 0 },
        }));
      } else {
        console.log();
        console.log(`  ${red('✗')}  No skill specified.`);
        console.log();
        console.log(`  ${dim('Usage:')}  octocode-skills install <name>`);
        console.log(`           octocode-skills install --all`);
        console.log(`  ${dim('Browse:')} octocode-skills list`);
        console.log();
      }
      process.exitCode = 1;
      return;
    }

    skills = [];
    const notFound: string[] = [];

    for (const name of skillNames) {
      const skill = getSkill(name);
      skill ? skills.push(skill) : notFound.push(name);
    }

    if (notFound.length > 0) {
      const msg = `Skill(s) not found: ${notFound.map((n) => `"${n}"`).join(', ')}`;
      if (opts.json) {
        console.log(JSON.stringify({ success: false, error: msg, skills: [], summary: { installed: 0, skipped: 0, failed: 0 } }));
      } else {
        console.log();
        console.log(`  ${red('✗')}  ${msg}`);
        console.log(`  ${dim('Run')} ${cyan('octocode-skills list')} ${dim('to browse available skills.')}`);
        console.log();
      }
      process.exitCode = 1;
      return;
    }
  }

  // ── Resolve platforms ─────────────────────────────────────────────────────

  let platforms: Platform[] = [];

  if (opts.platform) {
    const parsed = parsePlatforms(opts.platform);
    if (parsed.error) {
      if (opts.json) {
        console.log(JSON.stringify({ success: false, error: parsed.error }));
      } else {
        console.log(`\n  ${red('✗')}  ${parsed.error}\n`);
      }
      process.exitCode = 1;
      return;
    }
    platforms = parsed.platforms;
  }

  // ── Header (human) ────────────────────────────────────────────────────────

  if (!opts.json) {
    console.log();
    const modeLabel = opts.dryRun
      ? cyan('Dry-run preview') + dim(' — no files written')
      : force
        ? dim('mode: override')
        : dim('mode: keep existing (--keep)');
    console.log(`  ${bold('Installing')} ${dim(`${skills.length} skill(s)  ·  ${modeLabel}`)}`);
    console.log();
  }

  // ── Run installs ──────────────────────────────────────────────────────────

  const outcomes: SkillInstallOutcome[] = [];
  const spinner = opts.json ? null : new Spinner('').start();

  for (const skill of skills) {
    spinner?.update(`Installing ${skill.name}…`);

    const outcome = installSkill({
      sourcePath: skill.dir,
      skillName: skill.folder,
      platforms,
      workspace: opts.workspace,
      customPath: opts.customPath,
      mode: opts.mode,
      force,
      dryRun: opts.dryRun,
    });
    outcomes.push(outcome);
  }

  spinner?.stop();

  // ── Totals ────────────────────────────────────────────────────────────────

  let installed = 0, skipped = 0, failed = 0;

  for (const o of outcomes) {
    if (o.homeStatus === 'installed') installed++;
    else if (o.homeStatus === 'skipped') skipped++;
    else if (o.homeStatus === 'failed') failed++;

    for (const link of o.links) {
      if (link.status === 'linked') installed++;
      else if (link.status === 'skipped') skipped++;
      else if (link.status === 'failed') failed++;
    }
  }

  const success = failed === 0;

  // ── JSON output ───────────────────────────────────────────────────────────

  if (opts.json) {
    const result: InstallJsonResult = {
      success,
      dryRun: opts.dryRun,
      override: force,
      skills: outcomes.map((o) => ({
        name: o.skillName,
        home: o.homePath,
        homeStatus: o.homeStatus,
        ...(o.homeError ? { homeError: o.homeError } : {}),
        links: o.links.map((l) => ({
          target: l.target,
          destPath: l.destPath,
          status: l.status,
          ...(l.error ? { error: l.error } : {}),
        })),
      })),
      summary: { installed, skipped, failed },
    };
    console.log(JSON.stringify(result, null, 2));
    if (!success) process.exitCode = 1;
    return;
  }

  // ── Human output ──────────────────────────────────────────────────────────

  for (const o of outcomes) {
    const skillOk = o.homeStatus === 'installed' || o.homeStatus === 'skipped' || o.homeStatus === 'bypassed';
    const overallIcon = skillOk && o.links.every((l) => l.status !== 'failed')
      ? green('✓')
      : red('✗');

    console.log(`  ${overallIcon}  ${bold(o.skillName)}`);

    // Home row
    if (o.homeStatus !== 'bypassed') {
      const icon = statusIcon(o.homeStatus);
      const dest = o.homePath ? dim(shortPath(o.homePath)) : dim('—');

      let note = '';
      if (o.homeStatus === 'skipped') {
        note = dim('  (kept existing  ·  remove --keep to override)');
      } else if (o.homeStatus === 'installed' && force && !opts.dryRun) {
        note = dim('  (overwritten)');
      } else if (o.homeError) {
        note = red(`  ${o.homeError}`);
      }

      console.log(`     ${icon}  ${'home'.padEnd(14)} ${dest}${note}`);
    }

    // Link rows
    for (const link of o.links) {
      const icon = statusIcon(link.status);
      const dest = dim(shortPath(link.destPath));

      let note = '';
      if (link.status === 'skipped') {
        note = dim('  (kept existing)');
      } else if (link.status === 'linked') {
        note = dim('  → ' + shortPath(o.homePath ?? link.destPath));
      } else if (link.error) {
        note = red(`  ${link.error}`);
      }

      console.log(`     ${icon}  ${link.target.padEnd(14)} ${dest}${note}`);
    }

    console.log();
  }

  // Summary
  const homeDir = opts.customPath ? shortPath(opts.customPath) : shortPath(getSkillsHome());
  const summaryParts = [
    installed > 0 ? green(`${installed} installed`) : null,
    skipped > 0 ? yellow(`${skipped} kept`) : null,
    failed > 0 ? red(`${failed} failed`) : null,
  ].filter(Boolean).join('  ·  ');

  console.log(`  ${dim('─'.repeat(60))}`);
  console.log(`  ${summaryParts}`);
  console.log(`  ${dim('Home:')} ${homeDir}`);
  if (platforms.length > 0) {
    console.log(`  ${dim('Platforms:')} ${platforms.join(', ')}`);
  }
  console.log();

  // Failure notice
  if (!success) {
    console.log(`  ${red('Some installations failed.')} Check the errors above.`);
    console.log();
    process.exitCode = 1;
    return;
  }

  // Post-install tips (only when something was actually installed)
  if (installed > 0 && !opts.dryRun) {
    if (!opts.platform) {
      console.log(`  ${dim('Tip:')} Link to an agent dir with ${cyan('--platform')}:`);
      const eg = skills[0]?.name ?? '<name>';
      console.log(`  ${dim('  e.g.')} ${cyan(`octocode-skills install ${eg} --platform pi`)}`);
      console.log(`  ${dim('  or')}  ${cyan(`octocode-skills install --all --platform pi,cursor`)}`);
      console.log();
    }
    console.log(`  ${dim('Verify:')} ${cyan('octocode-skills check')}`);
    console.log();

    // Env param warnings
    const envStatuses = getSkillsEnvStatus(skills.map((s) => s.folder));
    const needsEnv = envStatuses.filter(
      (e) => e.readiness === 'needs-config' || e.readiness === 'partial'
    );

    if (needsEnv.length > 0) {
      const verb = needsEnv.some((e) => e.readiness === 'needs-config') ? red('⚠ Env required') : yellow('⚠ Env recommended');
      console.log(`  ${verb} — some skills need env configuration:`);
      console.log();

      const shownGroups = new Set<string>();
      const shownKeys = new Set<string>();

      for (const env of needsEnv) {
        for (const ps of env.params) {
          if (ps.status === 'set') continue;
          if (isGroupSatisfied(ps, env.params)) continue;
          const { group, key, required, link } = ps.param;

          if (group) {
            if (shownGroups.has(group)) continue;
            shownGroups.add(group);
            const groupKeys = env.params.filter((p) => p.param.group === group);
            const affectedSkills = needsEnv
              .filter((e2) => e2.params.some((p) => p.param.group === group && !isGroupSatisfied(p, e2.params)))
              .map((e2) => e2.skillName);
            console.log(`  ${dim(groupLabel(group))}  ${dim(`[${required}]`)}  ${dim(`→ ${[...new Set(affectedSkills)].join(', ')}`)}`)
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

      console.log(`  ${dim('Add to')} ~/.octocode/.env  ${dim('then')} ${cyan('octocode-skills check')}`);
      console.log();
    }
  }
}

