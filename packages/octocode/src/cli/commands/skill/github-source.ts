import path from 'node:path';
import type {
  MarketplaceSkill,
  MarketplaceSource,
} from '../../../configs/skills-marketplace.js';
import { isSafeSkillName } from '../../../utils/skills.js';
import { formatSkillName, slugify } from './naming.js';
import { OCTOCODE_SKILLS_GITHUB, OCTOCODE_SKILLS_SOURCE } from './types.js';
import type { GithubSkillFolder } from './types.js';

export function stripSkillMd(input: string): string {
  return input
    .replace(/\/SKILL\.md$/i, '')
    .replace(/^SKILL\.md$/i, '')
    .replace(/^\/+|\/+$/g, '');
}

export function parseGitHubUrl(
  rawInput: string,
  branchOverride?: string
): GithubSkillFolder | null {
  let url: URL;
  try {
    url = new URL(rawInput);
  } catch {
    return null;
  }

  if (url.hostname !== 'github.com') {
    return null;
  }

  const parts = url.pathname.split('/').filter(Boolean);
  const [owner, rawRepo, kind, ...rest] = parts;
  if (!owner || !rawRepo) {
    return null;
  }

  const repo = rawRepo.replace(/\.git$/i, '');
  if (kind === 'tree' || kind === 'blob') {
    const branch = branchOverride ?? rest[0] ?? 'main';
    const skillPath = stripSkillMd(rest.slice(1).join('/'));
    return {
      owner,
      repo,
      branch,
      skillPath,
      url: rawInput,
    };
  }

  return {
    owner,
    repo,
    branch: branchOverride ?? 'main',
    skillPath: '',
    url: rawInput,
  };
}

export function parseGitHubShorthand(
  rawInput: string,
  branchOverride?: string
): GithubSkillFolder | null {
  const cleaned = rawInput.replace(/^github:/i, '').replace(/^\/+|\/+$/g, '');
  const parts = cleaned.split('/').filter(Boolean);
  const [owner, rawRepo, maybeKind, ...rest] = parts;
  if (!owner || !rawRepo) {
    return null;
  }

  const [repoName, inlineBranch] = rawRepo.replace(/\.git$/i, '').split('@');
  const repo = repoName;
  if (!repo) {
    return null;
  }

  if (maybeKind === 'tree' || maybeKind === 'blob') {
    const branch = branchOverride ?? rest[0] ?? inlineBranch ?? 'main';
    return {
      owner,
      repo,
      branch,
      skillPath: stripSkillMd(rest.slice(1).join('/')),
      url: `https://github.com/${owner}/${repo}/tree/${branch}/${stripSkillMd(
        rest.slice(1).join('/')
      )}`,
    };
  }

  const branch = branchOverride ?? inlineBranch ?? 'main';
  const skillPath = stripSkillMd(parts.slice(2).join('/'));
  return {
    owner,
    repo,
    branch,
    skillPath,
    url: `https://github.com/${owner}/${repo}/tree/${branch}/${skillPath}`,
  };
}

export function parseGitHubSkillFolder(
  rawInput: string,
  branchOverride?: string
): GithubSkillFolder | null {
  return (
    parseGitHubUrl(rawInput, branchOverride) ??
    parseGitHubShorthand(rawInput, branchOverride)
  );
}

export function buildOctocodeSkillFolder(
  skillName: string,
  branchOverride?: string
): GithubSkillFolder | null {
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  const branch = branchOverride ?? OCTOCODE_SKILLS_GITHUB.branch;
  const skillPath = `${OCTOCODE_SKILLS_GITHUB.skillsPath}/${skillName}`;
  return {
    owner: OCTOCODE_SKILLS_GITHUB.owner,
    repo: OCTOCODE_SKILLS_GITHUB.repo,
    branch,
    skillPath,
    url: `https://github.com/${OCTOCODE_SKILLS_GITHUB.owner}/${OCTOCODE_SKILLS_GITHUB.repo}/tree/${branch}/${skillPath}`,
  };
}

export function buildMarketplaceSkill(
  ref: GithubSkillFolder
): MarketplaceSkill | null {
  const skillName = path.posix.basename(ref.skillPath || ref.repo);
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  const sourceId = slugify(
    ['github', ref.owner, ref.repo, ref.branch, ref.skillPath || 'root'].join(
      '-'
    )
  );

  return {
    name: skillName,
    displayName: formatSkillName(skillName),
    description: `GitHub skill folder ${ref.owner}/${ref.repo}/${ref.skillPath}`,
    path: ref.skillPath,
    source: {
      id: sourceId,
      name: `${ref.owner}/${ref.repo}`,
      type: 'github',
      owner: ref.owner,
      repo: ref.repo,
      branch: ref.branch,
      skillsPath: ref.skillPath ? path.posix.dirname(ref.skillPath) : '',
      skillPattern: 'skill-folders',
      description: 'GitHub skill folder',
      url: ref.url,
    },
  };
}

export function buildGitHubSourceUrl(skill: MarketplaceSkill): string {
  const source = skill.source;
  return `https://github.com/${source.owner}/${source.repo}/tree/${source.branch}/${skill.path}`;
}

export function buildOctocodeSkillsSource(
  branchOverride?: string
): MarketplaceSource {
  const branch = branchOverride ?? OCTOCODE_SKILLS_GITHUB.branch;
  return {
    ...OCTOCODE_SKILLS_SOURCE,
    branch,
    id: `github-${OCTOCODE_SKILLS_GITHUB.owner}-${OCTOCODE_SKILLS_GITHUB.repo}-${branch}-${OCTOCODE_SKILLS_GITHUB.skillsPath}`,
    url: `https://github.com/${OCTOCODE_SKILLS_GITHUB.owner}/${OCTOCODE_SKILLS_GITHUB.repo}/tree/${branch}/${OCTOCODE_SKILLS_GITHUB.skillsPath}`,
  };
}

export function buildGitHubLibrarySource(
  ref: GithubSkillFolder
): MarketplaceSource {
  const sourceId = slugify(
    ['github', ref.owner, ref.repo, ref.branch, ref.skillPath || 'root'].join(
      '-'
    )
  );

  return {
    id: sourceId,
    name: `${ref.owner}/${ref.repo}`,
    type: 'github',
    owner: ref.owner,
    repo: ref.repo,
    branch: ref.branch,
    skillsPath: ref.skillPath,
    skillPattern: 'skill-folders',
    description: `GitHub skills library ${ref.owner}/${ref.repo}/${ref.skillPath}`,
    url: ref.url,
  };
}
