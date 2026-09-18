import { listSkills } from '../registry.js';
import {
  checkSkill,
  isInstalledAtHome,
  linkedPlatforms,
  hasBroken,
} from '../checker.js';
import { getSkillEnvStatus, missingHint } from '../env-params.js';
import { bold, c, dim } from '../../../../utils/colors.js';

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

export function runList(opts: { json: boolean }): void {
  const bundled = listSkills();
  const skills = bundled.map(skill => {
    const check = checkSkill(skill.folder);
    const env = getSkillEnvStatus(skill.folder);
    return {
      name: skill.name,
      folder: skill.folder,
      description: skill.description,
      installed: isInstalledAtHome(check),
      linkedPlatforms: linkedPlatforms(check),
      hasWorkspaceLink:
        check.workspace.status === 'linked' ||
        check.workspace.status === 'installed',
      hasBroken: hasBroken(check),
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
    installedCount: skills.filter(skill => skill.installed).length,
    skills,
  };

  if (opts.json) {
    console.log(JSON.stringify(result, null, 2));
    return;
  }
  console.log(
    `\n  ${bold('Octocode skills')} ${dim(`· ${result.installedCount}/${result.count} installed`)}`
  );
  if (skills.length === 0) console.log('  No bundled skills found.');
  for (const skill of skills) {
    const icon = skill.hasBroken
      ? c('yellow', '⚠')
      : skill.installed
        ? c('green', '✓')
        : dim('–');
    const links =
      skill.linkedPlatforms.length > 0
        ? ` · ${skill.linkedPlatforms.join(', ')}`
        : '';
    console.log(`  ${icon} ${skill.name}${dim(links)} — ${skill.description}`);
    if (skill.env.hint) console.log(`    ${dim(`env: ${skill.env.hint}`)}`);
  }
  console.log();
}
