/**
 * @octocodeai/skills — programmatic API
 */

// Registry — read skill metadata from bundled skills
export { listSkills, getSkill, getSkillContent, getBundledSkillsDir } from './registry.js';
export type { SkillInfo } from './registry.js';

// Home — resolve canonical skills directory
export { getOctocodeHome, getSkillsHome } from './home.js';

// Platforms — resolve platform-specific skill directories
export { getPlatformSkillsDir, parsePlatforms, VALID_PLATFORMS } from './platforms.js';
export type { Platform } from './platforms.js';

// Installer — install skills programmatically
export { installSkill } from './installer.js';
export type { InstallMode, InstallSkillParams, SkillInstallOutcome, LinkResult } from './installer.js';

// Remove — uninstall skills programmatically
export { runRemove } from './commands/remove.js';
export type { RemoveOptions } from './commands/remove.js';

// Checker — verify installation status
export {
  checkSkill,
  checkSkills,
  isInstalledAtHome,
  linkedPlatforms,
  hasBroken,
  overallStatus,
  SCAN_PLATFORMS,
} from './checker.js';
export type { CheckedLocation, SkillCheckResult, LocationStatus } from './checker.js';
