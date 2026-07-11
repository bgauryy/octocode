import { c, bold, dim } from '../../../utils/colors.js';
import { input } from '../../../utils/prompts.js';
import { dirExists } from '../../../utils/fs.js';
import { getSkillsDestDir } from '../../../utils/skills.js';
import {
  type MarketplaceSource,
  type MarketplaceSkill,
  isLocalSource,
} from '../../../configs/skills-marketplace.js';
import path from 'node:path';

export type MarketplaceMenuChoice = MarketplaceSource | 'back';
export type SkillMenuChoice = MarketplaceSkill | 'back';
export type InstallChoice = 'install' | 'delete' | 'back';
export type OfficialFlowChoice = 'install-all' | 'browse' | 'back';

export const RECOMMENDED_SKILLS = new Set(['octocode-research']);

export async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

export function formatMarketplace(
  source: MarketplaceSource,
  stars?: number
): string {
  if (isLocalSource(source)) {
    return `${bold(source.name)} ${c('cyan', 'bundled')} - ${dim(source.description)}`;
  }
  const starsText = stars ? ` ${stars.toLocaleString()}` : '';
  return `${bold(source.name)}${c('yellow', starsText)} - ${dim(source.description)}`;
}

export function formatSkill(
  skill: MarketplaceSkill,
  installed: boolean
): string {
  const installedTag = installed ? c('green', '✅ ') : '';
  const starTag = RECOMMENDED_SKILLS.has(skill.name) ? c('yellow', ' *') : '';
  const desc = skill.description.slice(0, 50);
  const ellipsis = skill.description.length > 50 ? '...' : '';
  return `${installedTag}${skill.displayName}${starTag} ${dim(desc)}${dim(ellipsis)}`;
}

export function isSkillInstalled(skillName: string): boolean {
  const destDir = getSkillsDestDir();
  return dirExists(path.join(destDir, skillName));
}
