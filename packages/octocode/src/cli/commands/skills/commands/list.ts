/**
 * `octocode skill list`
 *
 * Lists all bundled skills with install status AND env param readiness.
 *
 *   ✓  installed (+ platform links)          env: ✓ ready / ⚠ partial / ✗ needs-config
 *   –  not installed                          env: … (same)
 *
 * Use --json for agent-parseable output.
 */

import { listSkills } from '../registry.js';
import {
  checkSkill,
  isInstalledAtHome,
  linkedPlatforms,
  hasBroken,
} from '../checker.js';
import {
  getSkillEnvStatus,
  missingHint,
  groupLabel,
  type SkillEnvStatus,
} from '../env-params.js';
import { bold, dim, c } from '../../../../utils/colors.js';

// ─── JSON shape ───────────────────────────────────────────────────────────────

export interface ListResult {
  success: boolean;
  skills: Array<{
    name: string;
    folder: string;
    description: string;
    installed: boolean;
    linkedPlatforms: string[];
    hasWorkspaceLink: boolean;
    hasBroken: boolean;
    env: {
      readiness: string;
      params: Array<{
        key: string;
        status: string;
        required: string;
        group?: string;
      }>;
      hint: string;
    };
  }>;
  source: string;
  count: number;
  installedCount: number;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function installBadge(installed: boolean, broken: boolean): string {
  if (broken) return c('yellow', '⚠');
  if (installed) return c('green', '✓');
  return dim('–');
}

function envBadge(readiness: SkillEnvStatus['readiness']): string {
  switch (readiness) {
    case 'ok':
    case 'ready':
      return c('green', '✓');
    case 'partial':
      return c('yellow', '⚠');
    case 'needs-config':
      return c('red', '✗');
  }
}

function envReadinessLabel(readiness: SkillEnvStatus['readiness']): string {
  switch (readiness) {
    case 'ok':
      return dim('no env needed');
    case 'ready':
      return c('green', 'env ready');
    case 'partial':
      return c('yellow', 'env partial');
    case 'needs-config':
      return c('red', 'env missing');
  }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export function runList(opts: { json: boolean }): void {
  const skills = listSkills();

  if (skills.length === 0) {
    if (opts.json) {
      console.log(
        JSON.stringify({
          success: true,
          skills: [],
          source: 'bundled',
          count: 0,
          installedCount: 0,
        })
      );
    } else {
      console.log('\n  No skills found in bundled skills directory.\n');
    }
    return;
  }

  const checks = skills.map(s => checkSkill(s.folder));
  const envStatuses = skills.map(s => getSkillEnvStatus(s.folder));

  if (opts.json) {
    const result: ListResult = {
      success: true,
      source: 'bundled',
      count: skills.length,
      installedCount: checks.filter(isInstalledAtHome).length,
      skills: skills.map((s, i) => {
        const check = checks[i]!;
        const env = envStatuses[i]!;
        return {
          name: s.name,
          folder: s.folder,
          description: s.description,
          installed: isInstalledAtHome(check),
          linkedPlatforms: linkedPlatforms(check),
          hasWorkspaceLink:
            check.workspace.status === 'linked' ||
            check.workspace.status === 'installed',
          hasBroken: hasBroken(check),
          env: {
            readiness: env.readiness,
            params: env.params.map(ps => ({
              key: ps.param.key,
              status: ps.status,
              required: ps.param.required,
              ...(ps.param.group ? { group: ps.param.group } : {}),
            })),
            hint: missingHint(env),
          },
        };
      }),
    };
    console.log(JSON.stringify(result, null, 2));
    return;
  }

  // ── Human output ──────────────────────────────────────────────────────────

  const installedCount = checks.filter(isInstalledAtHome).length;
  const brokenCount = checks.filter(hasBroken).length;
  const needsConfigCount = envStatuses.filter(
    e => e.readiness === 'needs-config'
  ).length;
  const partialCount = envStatuses.filter(
    e => e.readiness === 'partial'
  ).length;

  const homeBase = process.env['HOME'] ?? '';
  const shortHome = (p: string) => (homeBase ? p.replace(homeBase, '~') : p);

  const nameWidth = Math.max(...skills.map(s => s.name.length)) + 2;

  console.log();

  const counters = [
    `${skills.length} bundled`,
    `${installedCount} installed`,
    ...(brokenCount ? [c('yellow', `${brokenCount} broken`)] : []),
    ...(needsConfigCount ? [c('red', `${needsConfigCount} needs env`)] : []),
    ...(partialCount && !needsConfigCount
      ? [c('yellow', `${partialCount} partial env`)]
      : []),
  ];
  console.log(
    `  ${bold('Octocode skills')}  ${dim('·')} ${dim(counters.join(' · '))}`
  );
  console.log();

  for (let i = 0; i < skills.length; i++) {
    const skill = skills[i]!;
    const check = checks[i]!;
    const env = envStatuses[i]!;

    const installed = isInstalledAtHome(check);
    const broken = hasBroken(check);
    const installIcon = installBadge(installed, broken);

    // Install location hint
    const linked = linkedPlatforms(check);
    const hasWs =
      check.workspace.status === 'linked' ||
      check.workspace.status === 'installed';
    let locationHint = '';
    if (broken) {
      locationHint = c('red', ' broken symlink');
    } else if (installed) {
      const parts: string[] = [];
      if (check.home.status === 'installed')
        parts.push(dim(shortHome(check.home.path)));
      if (linked.length) parts.push(dim(`+ ${linked.join(', ')}`));
      if (hasWs) parts.push(dim('+ workspace'));
      locationHint = parts.length ? `  ${parts.join('  ')}` : '';
    }

    // Env status
    const eIcon = envBadge(env.readiness);
    const eLabel = envReadinessLabel(env.readiness);
    const hint = missingHint(env);

    const namePart = installed
      ? c('cyan', skill.name.padEnd(nameWidth))
      : skill.name.padEnd(nameWidth);
    const descPart = dim(
      skill.description.slice(0, 68) +
        (skill.description.length > 68 ? '…' : '')
    );

    // Row 1: install badge + name + location
    console.log(`  ${installIcon}  ${namePart}${locationHint}`);
    // Row 2: description
    console.log(`     ${descPart}`);
    // Row 3: env status (skip if no params and ok)
    if (env.readiness !== 'ok') {
      if (hint) {
        console.log(`     ${eIcon} env  ${eLabel}  ${dim('·')}  ${dim(hint)}`);
      } else {
        console.log(`     ${eIcon} env  ${eLabel}`);
      }
    }
    console.log();
  }

  // Footer
  console.log(`  ${dim('─'.repeat(60))}`);

  if (installedCount === 0) {
    console.log(`  ${dim('None installed.')} Get started:`);
    console.log();
    console.log(
      `  ${c('cyan', 'octocode skill install --all')}                 install all skills`
    );
    console.log(
      `  ${c('cyan', 'octocode skill install octocode-research')}     install one skill`
    );
  } else {
    console.log(
      `  ${dim('Add a platform link:')}  ${c('cyan', 'octocode skill install <name> --platform pi')}`
    );
    console.log(
      `  ${dim('Add workspace link:')}   ${c('cyan', 'octocode skill install <name> --workspace')}`
    );
    console.log(
      `  ${dim('Check installs:')}       ${c('cyan', 'octocode skill check')}`
    );
  }

  if (needsConfigCount > 0 || partialCount > 0) {
    console.log();
    console.log(`  ${dim('─'.repeat(60))}`);
    if (needsConfigCount > 0) {
      console.log(
        `  ${c('red', '⚠')} ${needsConfigCount} skill(s) need env configuration — add to ${dim('~/.octocode/.env')}:`
      );
    } else {
      console.log(
        `  ${c('yellow', '⚠')} ${partialCount} skill(s) have partial env — more keys = better results:`
      );
    }
    console.log();

    // Show which keys are missing across all skills
    const missingKeys = new Map<
      string,
      { param: import('../env-params.js').EnvParam; skills: string[] }
    >();
    for (const env of envStatuses) {
      for (const ps of env.params) {
        if (ps.status === 'missing') {
          const existing = missingKeys.get(ps.param.key);
          if (existing) {
            existing.skills.push(env.skillName);
          } else {
            missingKeys.set(ps.param.key, {
              param: ps.param,
              skills: [env.skillName],
            });
          }
        }
      }
    }

    // Deduplicate groups — only show each group once
    const shownGroups = new Set<string>();
    for (const [key, { param, skills: affectedSkills }] of missingKeys) {
      // For grouped params, only show the group once
      if (param.group) {
        if (shownGroups.has(param.group)) continue;
        // Check if ANY param in the group is set — if so, skip showing this group
        const anySet = envStatuses.some(e =>
          e.params.some(
            ps => ps.param.group === param.group && ps.status === 'set'
          )
        );
        if (anySet) continue;
        shownGroups.add(param.group);
        const groupName = groupLabel(param.group);
        const uniqueSkills = [...new Set(affectedSkills)];
        console.log(
          `  ${dim('·')} ${bold(groupName)}  ${dim(`[${param.required}]`)}  ${dim(`→ ${uniqueSkills.join(', ')}`)}`
        );
        // Show each key in the group on sub-lines
        const groupParams = envStatuses
          .flatMap(e => e.params.filter(ps => ps.param.group === param.group))
          .filter(
            (ps, idx, arr) =>
              arr.findIndex(p => p.param.key === ps.param.key) === idx
          );
        for (const gps of groupParams) {
          const link = gps.param.link ? `  ${dim(gps.param.link)}` : '';
          console.log(`    ${dim(gps.param.key)}${link}`);
        }
      } else {
        const link = param.link ? `  ${dim(param.link)}` : '';
        console.log(
          `  ${dim('·')} ${bold(key)}  ${dim(`[${param.required}]`)}  ${dim(`→ ${[...new Set(affectedSkills)].join(', ')}`)}`
        );
        if (link) console.log(`    ${link}`);
      }
    }

    console.log();
    console.log(
      `  ${dim('Add to')} ${dim('~/.octocode/.env')}${dim(':')}  TAVILY_API_KEY=tvly-...`
    );
  }

  console.log();
  console.log(`  ${dim('install')} <name>      install (override by default)`);
  console.log(`  ${dim('install --all')}       install all bundled skills`);
  console.log(
    `  ${dim('info')} <name>         show full SKILL.md + env params`
  );
  console.log(
    `  ${dim('check')}               verify installations + env status`
  );
  console.log(`  ${dim('--json')}              machine-readable output`);
  console.log();
}
