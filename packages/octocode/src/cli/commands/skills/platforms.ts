import {
  SKILL_PLATFORMS,
  VALID_SKILL_PLATFORM_VALUES,
  resolveSkillDestination,
  type SkillPlatform,
} from '@octocodeai/octocode-skill-installer';

export const VALID_PLATFORMS: readonly string[] = VALID_SKILL_PLATFORM_VALUES;

export function getPlatformSkillsDir(platform: SkillPlatform): string {
  return resolveSkillDestination({ platform, scope: 'global' });
}

export const ALL_PLATFORMS: readonly SkillPlatform[] = SKILL_PLATFORMS.map(
  ({ platform }) => platform
);
