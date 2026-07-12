import type {
  MarketplaceSkill,
  MarketplaceSource,
} from '../../../configs/skills-marketplace.js';
import type {
  SkillInstallResult,
  SkillInstallStrategy,
  SkillInstallTarget,
} from '../../../utils/skills.js';
import type { EXIT } from '../../exit-codes.js';

export type GithubSkillFolder = {
  owner: string;
  repo: string;
  branch: string;
  skillPath: string;
  url: string;
};

export const OCTOCODE_SKILLS_GITHUB = {
  owner: 'bgauryy',
  repo: 'octocode',
  branch: 'main',
  skillsPath: 'skills',
} as const;

export const OCTOCODE_SKILLS_SOURCE: MarketplaceSource = {
  id: 'github-bgauryy-octocode-main-skills',
  name: 'bgauryy/octocode',
  type: 'github',
  owner: OCTOCODE_SKILLS_GITHUB.owner,
  repo: OCTOCODE_SKILLS_GITHUB.repo,
  branch: OCTOCODE_SKILLS_GITHUB.branch,
  skillsPath: OCTOCODE_SKILLS_GITHUB.skillsPath,
  skillPattern: 'skill-folders',
  description: 'Official Octocode skills',
  url: `https://github.com/${OCTOCODE_SKILLS_GITHUB.owner}/${OCTOCODE_SKILLS_GITHUB.repo}/tree/${OCTOCODE_SKILLS_GITHUB.branch}/${OCTOCODE_SKILLS_GITHUB.skillsPath}`,
};

// Last-resort fallback when both the bundled skills/ dir and the GitHub
// fetch are unavailable (e.g. running from source, unbuilt, and offline).
// Must mirror the skill folders under repo-root skills/ — keep in sync with
// `getAvailableSkills()`. 'octocode' and 'octocode-stats' are intentionally
// excluded: they are pi-extension-only meta skills with no folder under
// skills/, so they are not installable through this marketplace.
export const KNOWN_OCTOCODE_SKILLS = [
  'octocode-awareness',
  'octocode-research',
];

export const RECOMMENDED_SKILL = 'octocode-research';
export const DEFAULT_INSTALL_MODE: SkillInstallStrategy = 'symlink';

export type SkillInstallRequest = {
  skill: MarketplaceSkill;
  sourceUrl: string;
};

export type DestinationInstallResult = {
  target: SkillInstallTarget;
  destPath: string;
  result: SkillInstallResult;
};

export type SkillCommandResult = {
  skill: MarketplaceSkill;
  source: string;
  sourcePath: string;
  targets: DestinationInstallResult[];
  installed: number;
  skipped: number;
  failed: number;
  error?: string;
};

export type SkillRequestResolutionError = {
  error: string;
  status: typeof EXIT.NOT_FOUND | typeof EXIT.USAGE;
};

export type SkillRequestResolution =
  { requests: SkillInstallRequest[] } | SkillRequestResolutionError;
