import {
  listSkills,
  getSkill,
  retiredHint,
  RETIRED_SKILLS,
} from '../registry.js';
import {
  checkSkill,
  checkSkills,
  overallStatus,
  SCAN_PLATFORMS,
  type CheckedLocation,
  type SkillCheckResult,
} from '../checker.js';
import {
  getSkillEnvStatus,
  getSkillsEnvStatus,
  missingHint,
  envParamRows,
} from '../env-params.js';
import {
  parseSkillPlatforms,
  type SkillPlatform,
} from '@octocodeai/octocode-skill-installer';
import { getSkillsHome } from '../home.js';
import { installSkill } from '../installer.js';
import { runRemove } from './remove.js';
import { reportFailure } from './fail.js';
import { bold, c, dim } from '../../../../utils/colors.js';
import { statusIcon } from './list.js';

export interface CheckOptions {
  names: string[];
  platform: string | null;
  workspace: boolean;
  fix: boolean;
  dryRun: boolean;
  noEnv: boolean;
  json: boolean;
}

const fail = (message: string, json: boolean): void =>
  reportFailure(message, json, {
    human: `  ${c('red', '✗')} ${message}`,
  });

const needsRepair = (location: CheckedLocation): boolean =>
  location.status === 'broken' || location.content === 'stale';

/**
 * Repair only what is already there: refresh the canonical copy and relink
 * broken or stale locations. Never add platforms or a workspace the user did
 * not install into, and never replace a fresh link (e.g. a dev symlink).
 */
function fixSkill(
  result: SkillCheckResult,
  workspace: boolean,
  dryRun: boolean
): void {
  const skill = getSkill(result.skillName);
  if (!skill) return;
  const platforms = result.platforms
    .filter(needsRepair)
    .map(location => location.label as SkillPlatform);
  const repairWorkspace = workspace && needsRepair(result.workspace);
  if (dryRun) {
    const where = [
      'home',
      ...platforms,
      ...(repairWorkspace ? ['workspace'] : []),
    ].join(', ');
    console.log(
      `  ${dim('dry-run:')} would re-install ${result.skillName} (${overallStatus(result)}) → ${where}`
    );
    return;
  }
  installSkill({
    sourcePath: skill.dir,
    skillName: skill.folder,
    platforms,
    workspace: repairWorkspace,
    canonicalSkillsDir: getSkillsHome(),
    customPath: null,
    mode: 'symlink',
    force: true,
    dryRun: false,
  });
}

export function runCheck(opts: CheckOptions): void {
  const skillNames =
    opts.names.length > 0
      ? opts.names
      : listSkills().map(skill => skill.folder);
  const missing = skillNames.find(name => !getSkill(name));
  if (missing)
    return fail(
      `Skill not found: "${missing}".${retiredHint(missing)}`,
      opts.json);

  let platforms: SkillPlatform[] = SCAN_PLATFORMS;
  if (opts.platform) {
    const parsed = parseSkillPlatforms(opts.platform);
    if (parsed.error) return fail(parsed.error, opts.json);
    platforms = parsed.platforms;
  }

  let results = checkSkills(skillNames, platforms);
  if (opts.fix && !opts.json) {
    for (const result of results) {
      if (overallStatus(result) !== 'ok') {
        fixSkill(result, opts.workspace, opts.dryRun);
      }
    }
    if (!opts.dryRun) results = checkSkills(skillNames, platforms);
  }

  // A full check also finds installs of retired skills (real copies or links,
  // including links left dangling by the removal) and removes them on --fix.
  const findRetired = () =>
    opts.names.length > 0
      ? []
      : Object.entries(RETIRED_SKILLS)
          .map(([name, replacement]) => {
            const result = checkSkill(name, platforms);
            const paths = [result.home, ...result.platforms, result.workspace]
              .filter(location => location.status !== 'missing')
              .map(location => location.path);
            return { name, replacement, paths };
          })
          .filter(entry => entry.paths.length > 0);
  let retired = findRetired();
  if (opts.fix && !opts.json && retired.length > 0) {
    runRemove(
      retired.map(entry => entry.name),
      { all: false, platform: null, dryRun: opts.dryRun, json: false }
    );
    if (!opts.dryRun) retired = findRetired();
  }

  const envStatuses = opts.noEnv ? [] : getSkillsEnvStatus(skillNames);
  const statuses = results.map(overallStatus);
  const count = (status: string) =>
    statuses.filter(value => value === status).length;
  const envCount = (readiness: string) =>
    envStatuses.filter(value => value.readiness === readiness).length;
  const installOk =
    count('broken') === 0 && count('stale') === 0 && retired.length === 0;
  const envOk = opts.noEnv || envCount('needs-config') === 0;
  const success = installOk && envOk;

  const skills = results.map((result, index) => {
    const env = envStatuses[index] ?? getSkillEnvStatus(result.skillName);
    return {
      name: result.skillName,
      installStatus: overallStatus(result),
      home: {
        path: result.home.path,
        status: result.home.status,
        ...(result.home.linkTarget
          ? { linkTarget: result.home.linkTarget }
          : {}),
        ...(result.home.content ? { content: result.home.content } : {}),
      },
      platforms: result.platforms.map(location => ({
        label: location.label,
        path: location.path,
        status: location.status,
        ...(location.linkTarget ? { linkTarget: location.linkTarget } : {}),
        ...(location.content ? { content: location.content } : {}),
      })),
      workspace: {
        path: result.workspace.path,
        status: result.workspace.status,
        ...(result.workspace.linkTarget
          ? { linkTarget: result.workspace.linkTarget }
          : {}),
        ...(result.workspace.content
          ? { content: result.workspace.content }
          : {}),
      },
      env: {
        readiness: env.readiness,
        params: envParamRows(env),
        hint: missingHint(env),
      },
    };
  });
  const summary = {
    install: {
      ok: count('ok'),
      broken: count('broken'),
      stale: count('stale'),
      notInstalled: count('not-installed'),
      retired: retired.length,
      total: results.length,
    },
    env: opts.noEnv
      ? {
          ready: 0,
          partial: 0,
          needsConfig: 0,
          noParamsNeeded: skillNames.length,
        }
      : {
          ready: envCount('ready'),
          partial: envCount('partial'),
          needsConfig: envCount('needs-config'),
          noParamsNeeded: envCount('ok'),
        },
  };

  if (opts.json) {
    console.log(JSON.stringify({ success, skills, retired, summary }, null, 2));
  } else {
    console.log(bold('Skill check'));
    const width = Math.max(0, ...skills.map(skill => skill.name.length));
    for (const skill of skills) {
      const env = opts.noEnv ? '' : ` · env ${skill.env.readiness}`;
      console.log(
        `${statusIcon(skill.installStatus)} ${skill.name.padEnd(width)} ${skill.installStatus}${dim(env)}`
      );
    }
    for (const entry of retired) {
      console.log(
        `${c('red', '✗')} ${entry.name} retired → merged into ${entry.replacement} ${dim(entry.paths.join(', '))}`
      );
    }
    console.log(
      `${summary.install.ok} ok, ${summary.install.notInstalled} not installed, ${summary.install.stale} stale, ${summary.install.broken} broken, ${summary.install.retired} retired · env: ${summary.env.needsConfig} need config, ${summary.env.partial} optional missing`
    );
    if (!installOk && !opts.fix) {
      console.log(`${dim('Repair:')} ${c('cyan', 'octocode skill check --fix')}`);
    }
  }
  if (!success) process.exitCode = 1;
}
