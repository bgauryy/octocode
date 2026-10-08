import { listSkills, getSkill } from '../registry.js';
import {
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
  getCanonicalSkillsDir,
  installBundledSkills,
  parseSkillPlatforms,
  type SkillPlatform,
} from '@octocodeai/octocode-skill-installer';
import { reportFailure } from './fail.js';
import { EXIT } from '../../../exit-codes.js';
import { bold, c, dim } from '../../../../utils/colors.js';
import { statusIcon } from './list.js';

interface CheckOptions {
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
 * Returns the errors of a repair that did not complete.
 */
function fixSkill(
  result: SkillCheckResult,
  workspace: boolean,
  dryRun: boolean
): string[] {
  const skill = getSkill(result.skillName);
  if (!skill) return [];
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
    return [];
  }
  const repair = installBundledSkills({
    skills: [{ name: skill.folder, sourcePath: skill.dir }],
    canonicalSkillsDir: getCanonicalSkillsDir(),
    targets: [
      ...platforms.map(platform => ({ platform, scope: 'global' as const })),
      ...(repairWorkspace
        ? [
            {
              platform: 'codex' as const,
              scope: 'project' as const,
              projectDir: process.cwd(),
            },
          ]
        : []),
    ],
    mode: 'symlink',
    force: true,
  });
  if (repair.ok) return [];
  const outcome = repair.skills[0];
  const errors = [
    outcome?.canonicalError,
    ...(outcome?.destinations ?? []).map(destination =>
      destination.error
        ? `${destination.destination}: ${destination.error}`
        : destination.status === 'failed' || destination.status === 'conflict'
          ? `${destination.destination}: ${destination.status}`
          : undefined
    ),
  ].filter((error): error is string => Boolean(error));
  return errors.length > 0 ? errors : ['repair did not complete'];
}

const locationJson = (location: CheckedLocation) => ({
  path: location.path,
  status: location.status,
  ...(location.linkTarget ? { linkTarget: location.linkTarget } : {}),
  ...(location.content ? { content: location.content } : {}),
});

export function runCheck(opts: CheckOptions): void {
  const skillNames =
    opts.names.length > 0
      ? opts.names
      : listSkills().map(skill => skill.folder);
  const missing = skillNames.find(name => !getSkill(name));
  if (missing) return fail(`Skill not found: "${missing}".`, opts.json);

  let platforms: SkillPlatform[] = SCAN_PLATFORMS;
  if (opts.platform) {
    const parsed = parseSkillPlatforms(opts.platform);
    if (parsed.error) return fail(parsed.error, opts.json);
    platforms = parsed.platforms;
  }

  let results = checkSkills(skillNames, platforms);
  const repairFailures: Array<{ name: string; errors: string[] }> = [];
  if (opts.fix && !opts.json) {
    for (const result of results) {
      if (overallStatus(result) !== 'ok') {
        const errors = fixSkill(result, opts.workspace, opts.dryRun);
        if (errors.length > 0)
          repairFailures.push({ name: result.skillName, errors });
      }
    }
    if (!opts.dryRun) results = checkSkills(skillNames, platforms);
  }

  const envStatuses = opts.noEnv ? [] : getSkillsEnvStatus(skillNames);
  const statuses = results.map(overallStatus);
  const count = (status: string) =>
    statuses.filter(value => value === status).length;
  const envCount = (readiness: string) =>
    envStatuses.filter(value => value.readiness === readiness).length;
  const installOk =
    count('broken') === 0 &&
    count('stale') === 0 &&
    repairFailures.length === 0;
  const envOk = opts.noEnv || envCount('needs-config') === 0;
  const success = installOk && envOk;

  const skills = results.map((result, index) => {
    const env = envStatuses[index] ?? getSkillEnvStatus(result.skillName);
    return {
      name: result.skillName,
      installStatus: overallStatus(result),
      home: locationJson(result.home),
      platforms: result.platforms.map(location => ({
        label: location.label,
        ...locationJson(location),
      })),
      workspace: locationJson(result.workspace),
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
    console.log(JSON.stringify({ success, skills, summary }, null, 2));
  } else {
    console.log(bold('Skill check'));
    const width = Math.max(0, ...skills.map(skill => skill.name.length));
    for (const skill of skills) {
      const env = opts.noEnv ? '' : ` · env ${skill.env.readiness}`;
      console.log(
        `${statusIcon(skill.installStatus)} ${skill.name.padEnd(width)} ${skill.installStatus}${dim(env)}`
      );
    }
    for (const failure of repairFailures) {
      console.error(
        `${c('red', '✗')} ${failure.name} repair failed: ${failure.errors.join('; ')}`
      );
    }
    console.log(
      `${summary.install.ok} ok, ${summary.install.notInstalled} not installed, ${summary.install.stale} stale, ${summary.install.broken} broken · env: ${summary.env.needsConfig} need config, ${summary.env.partial} optional missing`
    );
    if (!installOk && !opts.fix) {
      console.log(
        `${dim('Repair:')} ${c('cyan', 'octocode skill check --fix')}`
      );
    }
  }
  if (!success) process.exitCode = EXIT.GENERAL;
}
