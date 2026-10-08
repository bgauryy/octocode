import {
  SKILL_PLATFORMS,
  resolveSkillDestination,
  type SkillPlatform,
} from '@octocodeai/octocode-skill-installer';

export function getPlatformSkillsDir(platform: SkillPlatform): string {
  return resolveSkillDestination({ platform, scope: 'global' });
}

export const ALL_PLATFORMS: readonly SkillPlatform[] = SKILL_PLATFORMS.map(
  ({ platform }) => platform
);
