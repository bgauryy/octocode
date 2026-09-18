import { listSkills, getSkill } from '../registry.js';
import {
  checkSkills,
  overallStatus,
  SCAN_PLATFORMS,
  type SkillCheckResult,
} from '../checker.js';
import {
  getSkillEnvStatus,
  getSkillsEnvStatus,
  missingHint,
  isGroupSatisfied,
} from '../env-params.js';
import { parsePlatforms, type Platform } from '../platforms.js';
import { installSkill } from '../installer.js';
import { bold, c, dim } from '../../../../utils/colors.js';

export interface CheckOptions {
  names: string[];
  platform: string | null;
  workspace: boolean;
  fix: boolean;
  noEnv: boolean;
  json: boolean;
}

function fail(message: string, json: boolean): void {
  if (json) console.log(JSON.stringify({ success: false, error: message }));
  else console.error(`  ${c('red', '✗')} ${message}`);
  process.exitCode = 1;
}

function fixSkill(result: SkillCheckResult, platforms: Platform[]): void {
  const skill = getSkill(result.skillName);
  if (!skill) return;
  installSkill({
    sourcePath: skill.dir,
    skillName: skill.folder,
    platforms,
    workspace:
      result.workspace.status === 'missing' ||
      result.workspace.status === 'broken',
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
  if (missing) return fail(`Skill not found: "${missing}"`, opts.json);

  let platforms: Platform[] = SCAN_PLATFORMS;
  if (opts.platform) {
    const parsed = parsePlatforms(opts.platform);
    if (parsed.error) return fail(parsed.error, opts.json);
    platforms = parsed.platforms;
  }

  let results = checkSkills(skillNames, platforms);
  if (opts.fix && !opts.json) {
    for (const result of results) {
      if (overallStatus(result) !== 'ok') fixSkill(result, platforms);
    }
    results = checkSkills(skillNames, platforms);
  }

  const envStatuses = opts.noEnv ? [] : getSkillsEnvStatus(skillNames);
  const statuses = results.map(overallStatus);
  const count = (status: string) =>
    statuses.filter(value => value === status).length;
  const envCount = (readiness: string) =>
    envStatuses.filter(value => value.readiness === readiness).length;
  const installOk = count('broken') === 0;
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
      },
      platforms: result.platforms.map(location => ({
        label: location.label,
        path: location.path,
        status: location.status,
        ...(location.linkTarget ? { linkTarget: location.linkTarget } : {}),
      })),
      workspace: {
        path: result.workspace.path,
        status: result.workspace.status,
        ...(result.workspace.linkTarget
          ? { linkTarget: result.workspace.linkTarget }
          : {}),
      },
      env: {
        readiness: env.readiness,
        params: env.params.map(param => ({
          key: param.param.key,
          status: param.status,
          required: param.param.required,
          description: param.param.description,
          ...(param.param.group
            ? {
                group: param.param.group,
                groupSatisfied: isGroupSatisfied(param, env.params),
              }
            : {}),
          ...(param.param.link ? { link: param.param.link } : {}),
        })),
        hint: missingHint(env),
      },
    };
  });
  const summary = {
    install: {
      ok: count('ok'),
      broken: count('broken'),
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
    console.log(`\n  ${bold('Skill check')}`);
    for (const skill of skills) {
      const icon =
        skill.installStatus === 'ok' ? c('green', '✓') : c('red', '✗');
      const env = opts.noEnv ? '' : ` · env ${skill.env.readiness}`;
      console.log(`  ${icon} ${skill.name}: ${skill.installStatus}${dim(env)}`);
    }
    console.log(
      `  ${summary.install.ok}/${summary.install.total} installed; ${summary.install.broken} broken; ${summary.env.needsConfig} need env\n`
    );
  }
  if (!success) process.exitCode = 1;
}
