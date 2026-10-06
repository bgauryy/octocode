import { listSkills } from '../registry.js';
import {
  checkSkill,
  linkedPlatforms,
  overallStatus,
  type SkillStatus,
} from '../checker.js';
import { getSkillEnvStatus, missingHint } from '../env-params.js';
import { bold, c, dim } from '../../../../utils/colors.js';

export interface ListResult {
  success: boolean;
  skills: Array<{
    name: string;
    folder: string;
    description: string;
    status: SkillStatus;
    linkedPlatforms: string[];
    hasWorkspaceLink: boolean;
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

/** The icon of a skill status, shared with `skill check`. */
export function statusIcon(status: SkillStatus): string {
  if (status === 'ok') return c('green', '✓');
  if (status === 'not-installed') return dim('–');
  return c('yellow', '⚠');
}

export function runList(opts: { json: boolean }): void {
  const skills = listSkills().map(skill => {
    const check = checkSkill(skill.folder);
    const env = getSkillEnvStatus(skill.folder);
    return {
      name: skill.name,
      folder: skill.folder,
      description: skill.description,
      status: overallStatus(check),
      linkedPlatforms: linkedPlatforms(check),
      hasWorkspaceLink:
        check.workspace.status === 'linked' ||
        check.workspace.status === 'installed',
      env: {
        readiness: env.readiness,
        params: env.params.map(param => ({
          key: param.param.key,
          status: param.status,
          required: param.param.required,
          ...(param.param.group ? { group: param.param.group } : {}),
        })),
        hint: missingHint(env),
      },
    };
  });
  const result: ListResult = {
    success: true,
    source: 'bundled',
    count: skills.length,
    installedCount: skills.filter(skill => skill.status !== 'not-installed')
      .length,
    skills,
  };

  if (opts.json) {
    console.log(JSON.stringify(result, null, 2));
    return;
  }
  console.log(
    `${bold('Octocode skills')} ${dim(`· ${result.installedCount}/${result.count} installed`)}`
  );
  if (skills.length === 0) console.log('No bundled skills found.');
  const width = Math.max(0, ...skills.map(skill => skill.name.length));
  for (const skill of skills) {
    const state = skill.status === 'ok' ? '' : ` ${skill.status}`;
    const links =
      skill.linkedPlatforms.length > 0
        ? dim(` ${skill.linkedPlatforms.join(', ')}`)
        : '';
    console.log(
      `${statusIcon(skill.status)} ${skill.name.padEnd(width)}${state}${links}`
    );
    if (skill.env.hint) console.log(`  ${dim(`env: ${skill.env.hint}`)}`);
  }
  if (skills.some(skill => skill.status === 'stale' || skill.status === 'broken')) {
    console.log(
      `${dim('Repair:')} ${c('cyan', 'octocode skill check --fix')}`
    );
  }
  console.log(
    dim(`Details: octocode skill info <name> · status: octocode skill check`)
  );
}
