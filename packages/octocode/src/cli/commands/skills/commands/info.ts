import { getSkill, getSkillContent } from '../registry.js';
import { getSkillEnvStatus, isGroupSatisfied } from '../env-params.js';
import { bold, c, dim } from '../../../../utils/colors.js';

export function runInfo(
  skillName: string,
  opts: { json: boolean; jsonErrors?: boolean }
): void {
  const skill = getSkill(skillName);
  if (!skill) {
    const error = `Skill not found: "${skillName}". Run \`octocode skill list\` to see available skills.`;
    if (opts.jsonErrors)
      console.log(
        JSON.stringify({ kind: 'octocode.toolError', version: 1, error })
      );
    else if (opts.json) console.log(JSON.stringify({ success: false, error }));
    else console.error(`\n  ${c('red', '✗')} ${error}\n`);
    process.exitCode = 1;
    return;
  }

  const skillMd = getSkillContent(skill);
  const env = getSkillEnvStatus(skill.folder);
  const params = env.params.map(param => ({
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
  }));
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
