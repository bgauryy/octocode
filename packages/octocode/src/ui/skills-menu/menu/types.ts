export const RECOMMENDED_SKILLS = new Set(['octocode-research']);

export interface InstalledSkill {
  name: string;

  description: string;

  folder: string;

  path: string;

  isBundled: boolean;

  isRecommended: boolean;
}

export type SkillsMenuChoice = 'manage' | 'marketplace' | 'back';
export type ManageSkillsChoice = InstalledSkill | 'back';
export type SkillActionChoice = 'remove' | 'view' | 'back';
