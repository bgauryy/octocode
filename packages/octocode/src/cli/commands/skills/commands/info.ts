import { getSkill, getSkillContent, retiredHint } from '../registry.js';
import { envParamRows, getSkillEnvStatus } from '../env-params.js';
import { reportFailure } from './fail.js';
import { bold, c, dim } from '../../../../utils/colors.js';

export function runInfo(
  skillName: string,
  opts: { json: boolean; jsonErrors?: boolean }
): void {
  const skill = getSkill(skillName);
  if (!skill) {
    reportFailure(
      `Skill not found: "${skillName}". Run \`octocode skill list\` to see available skills.${retiredHint(skillName)}`,
      opts.json,
      opts.jsonErrors
    );
    return;
  }

  const skillMd = getSkillContent(skill);
  const env = getSkillEnvStatus(skill.folder);
  const params = envParamRows(env);
  if (opts.json) {
    console.log(
      JSON.stringify(
        {
          success: true,
          skill: {
            name: skill.name,
            folder: skill.folder,
            description: skill.description,
            dir: skill.dir,
            skillMd: skillMd ?? null,
            env: { readiness: env.readiness, params },
          },
        },
        null,
        2
      )
    );
    return;
  }

  console.log(`\n  ${bold(skill.name)}`);
  console.log(`  ${dim(skill.description)}`);
  console.log(`  Path: ${skill.dir}`);
  console.log(`  Env: ${env.readiness}`);
  for (const param of params) {
    console.log(
      `    ${param.status === 'set' ? c('green', '✓') : c('yellow', '–')} ${param.key} [${param.required}]`
    );
  }
  console.log(`\n${skillMd ?? dim('(SKILL.md not readable)')}\n`);
}
