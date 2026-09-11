import {
  SKILL_PLATFORMS,
  VALID_SKILL_PLATFORM_VALUES,
  parseSkillPlatforms,
  resolveSkillDestination,
  type SkillPlatform,
} from '@octocodeai/octocode-skill-installer';

export type Platform = SkillPlatform;

export const VALID_PLATFORMS: readonly string[] = VALID_SKILL_PLATFORM_VALUES;

export function getPlatformSkillsDir(platform: Platform): string {
  return resolveSkillDestination({ platform, scope: 'global' });
}

export function parsePlatforms(raw: string): {
  platforms: Platform[];
  error?: string;
} {
  return parseSkillPlatforms(raw);
}

export const ALL_PLATFORMS: readonly Platform[] = SKILL_PLATFORMS.map(
  ({ platform }) => platform
);
