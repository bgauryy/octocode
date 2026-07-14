/**
 * `octocode-skills info <name>`
 *
 * Shows full skill details — name, description, env params, and SKILL.md content.
 * Use --json for agent-parseable output.
 */

import { getSkill, getSkillContent } from '../registry.js';
import {
  getSkillEnvStatus,
  isGroupSatisfied,
  groupLabel,
  type SkillEnvStatus,
} from '../env-params.js';
import { bold, dim, green, yellow, red, cyan } from '../utils/colors.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function envStatusLine(env: SkillEnvStatus): string {
  switch (env.readiness) {
    case 'ok':           return dim('none needed');
    case 'ready':        return green('all set');
    case 'partial':      return yellow('partial — some recommended keys missing');
    case 'needs-config': return red('configuration required');
  }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export function runInfo(skillName: string, opts: { json: boolean }): void {
  const skill = getSkill(skillName);

  if (!skill) {
    const msg = `Skill not found: "${skillName}". Run \`octocode-skills list\` to see available skills.`;
    if (opts.json) {
      console.log(JSON.stringify({ success: false, error: msg }));
    } else {
      console.error(`\n  ${red('✗')} ${msg}\n`);
    }
    process.exitCode = 1;
    return;
  }

  const content = getSkillContent(skill);
  const env = getSkillEnvStatus(skill.folder);

  // ── JSON ───────────────────────────────────────────────────────────────────

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
            skillMd: content ?? null,
            env: {
              readiness: env.readiness,
              params: env.params.map((ps) => ({
                key: ps.param.key,
                status: ps.status,
                required: ps.param.required,
                description: ps.param.description,
                ...(ps.param.group
                  ? { group: ps.param.group, groupSatisfied: isGroupSatisfied(ps, env.params) }
                  : {}),
                ...(ps.param.link ? { link: ps.param.link } : {}),
              })),
            },
          },
        },
        null,
        2
      )
    );
    return;
  }

  // ── Human ──────────────────────────────────────────────────────────────────

  console.log();
  console.log(`  ${bold(skill.name)}`);
  console.log(`  ${dim('Folder:')}  ${skill.folder}`);
  console.log(`  ${dim('Path:')}    ${skill.dir}`);
  console.log();

  // Env params block
  if (env.readiness === 'ok') {
    console.log(`  ${dim('Env params:')}  ${dim('none needed')}`);
  } else {
    console.log(`  ${bold('Env params')}  ${dim('·')}  ${envStatusLine(env)}`);
    console.log();

    const shownGroups = new Set<string>();

    for (const ps of env.params) {
      const { group, key, description, required, link } = ps.param;

      if (group) {
        if (shownGroups.has(group)) continue;
        shownGroups.add(group);

        const anySet = env.params.some((p) => p.param.group === group && p.status === 'set');
        const groupIcon = anySet ? green('✓') : required === 'required' ? red('✗') : yellow('⚠');
        const groupStr = `${groupLabel(group)}  ${dim(`[${required} — at least one]`)}`;
        console.log(`  ${groupIcon}  ${anySet ? groupStr : yellow(groupStr)}`);

        for (const gp of env.params.filter((p) => p.param.group === group)) {
          const setStr = gp.status === 'set' ? green(' ✓ set') : dim(' – not set');
          const linkStr = gp.param.link ? `  ${dim(gp.param.link)}` : '';
          console.log(`       ${(gp.status === 'set' ? green(gp.param.key) : dim(gp.param.key)).padEnd(32)}${setStr}${linkStr}`);
        }
        console.log();
      } else {
        const icon = ps.status === 'set'
          ? green('✓')
          : required === 'required' ? red('✗') : yellow('⚠');
        const keyStr = ps.status === 'set' ? green(key) : yellow(key);
        const statusStr = ps.status === 'set' ? green('set') : dim(`not set  [${required}]`);
        const linkStr = link ? `  ${dim(link)}` : '';
        console.log(`  ${icon}  ${keyStr.padEnd(32)} ${statusStr}${linkStr}`);
        console.log(`       ${dim(description)}`);
        console.log();
      }
    }

    if (env.readiness !== 'ready') {
      console.log(`  ${dim('Add to')} ~/.octocode/.env${dim(':')}  KEY=value`);
      console.log(`  ${dim('Verify:')} ${cyan(`octocode-skills check ${skill.folder}`)}`);
      console.log();
    }
  }

  console.log(`  ${dim('─'.repeat(60))}`);
  console.log();

  // SKILL.md content
  if (content) {
    const lines = content.split('\n');
    for (const line of lines) {
      console.log(`  ${line}`);
    }
  } else {
    console.log(`  ${dim('(SKILL.md not readable)')}`);
  }

  console.log();
}
