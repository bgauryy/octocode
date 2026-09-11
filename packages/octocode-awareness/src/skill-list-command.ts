import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  AWARENESS_SKILL_NAME,
  bundledSkillPath,
  failSkillCommand,
  inspectCanonicalSkill,
  type SkillCommandArguments,
  type SkillCommandOptions,
  type SkillCommandResult,
} from './skill-command-utils.js';

function bundledDescription(skillFile: string): string {
  const markdown = readFileSync(skillFile, 'utf8');
  const frontmatter = markdown.match(/^---\r?\n([\s\S]*?)\r?\n---/u)?.[1] ?? '';
  const quoted = frontmatter.match(/^description:\s*["'](.+)["']\s*$/mu);
  if (quoted?.[1]) return quoted[1];
  return frontmatter.match(/^description:\s*(.+)\s*$/mu)?.[1]?.trim() ?? '';
}

export function runSkillList(
  _argv: SkillCommandArguments,
  options: SkillCommandOptions
): SkillCommandResult {
  const source = bundledSkillPath(options);
  const skillFile = join(source, 'SKILL.md');
  if (!existsSync(skillFile))
    return failSkillCommand(`bundled skill is missing: ${skillFile}`);

  const canonical = inspectCanonicalSkill(options);
  const installed =
    canonical.status === 'installed' || canonical.status === 'outdated';
  return {
    exitCode: 0,
    payload: {
      ok: true,
      action: 'list',
      source: 'bundled',
      count: 1,
      installedCount: installed ? 1 : 0,
      skills: [
        {
          name: AWARENESS_SKILL_NAME,
          source,
          canonical: canonical.canonical,
          canonicalStatus: canonical.status,
          installed,
          description: bundledDescription(skillFile),
        },
      ],
    },
  };
}
