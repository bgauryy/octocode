import { readdirSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import type { MarketplaceSource } from '../../../configs/skills-marketplace.js';
import { dirExists, fileExists } from '../../../utils/fs.js';
import { HOME } from '../../../utils/platform.js';
import { isSafeSkillName } from '../../../utils/skills.js';
import { EXIT } from '../../exit-codes.js';
import { formatSkillName, slugify } from './naming.js';
import type { SkillInstallRequest, SkillRequestResolution } from './types.js';

export function buildLocalSkillSource(sourceRoot: string): MarketplaceSource {
  const resolvedRoot = path.resolve(sourceRoot);
  return {
    id: slugify(['local', resolvedRoot].join('-')) || 'local-skills',
    name: 'Local skills',
    type: 'local',
    owner: '',
    repo: '',
    branch: '',
    skillsPath: resolvedRoot,
    skillPattern: 'skill-folders',
    description: 'Local skill folder',
    url: pathToFileURL(resolvedRoot).href,
  };
}

export function stripLocalSkillMd(rawPath: string): string {
  const trimmed = rawPath.trim();
  return path.basename(trimmed).toLowerCase() === 'skill.md'
    ? path.dirname(trimmed)
    : trimmed;
}

export function expandLocalPath(rawPath: string): string {
  const trimmed = rawPath.trim();
  if (trimmed === '~') return HOME;
  if (trimmed.startsWith('~/') || trimmed.startsWith('~\\')) {
    return path.join(HOME, trimmed.slice(2));
  }
  return trimmed;
}

export function buildLocalSkillRequest(
  skillDir: string
): SkillInstallRequest | null {
  const resolvedSkillDir = path.resolve(stripLocalSkillMd(skillDir));
  if (
    !dirExists(resolvedSkillDir) ||
    !fileExists(path.join(resolvedSkillDir, 'SKILL.md'))
  ) {
    return null;
  }

  const skillName = path.basename(resolvedSkillDir);
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  return {
    skill: {
      name: skillName,
      displayName: formatSkillName(skillName),
      description: 'Local skill folder',
      path: skillName,
      source: buildLocalSkillSource(path.dirname(resolvedSkillDir)),
    },
    sourceUrl: pathToFileURL(resolvedSkillDir).href,
  };
}

export function resolveLocalSkillRequests(
  rawPath: string
): SkillRequestResolution {
  const resolvedPath = path.resolve(
    stripLocalSkillMd(expandLocalPath(rawPath))
  );
  const directSkill = buildLocalSkillRequest(resolvedPath);
  if (directSkill) {
    return { requests: [directSkill] };
  }

  if (!dirExists(resolvedPath)) {
    return {
      error: `Local skill path not found: ${resolvedPath}`,
      status: EXIT.NOT_FOUND,
    };
  }

  let entries: string[] = [];
  try {
    entries = readdirSync(resolvedPath, { withFileTypes: true })
      .filter(entry => entry.isDirectory() || entry.isSymbolicLink())
      .map(entry => entry.name)
      .sort((a, b) => a.localeCompare(b));
  } catch (error) {
    return {
      error:
        error instanceof Error
          ? error.message
          : `Could not read local skill path: ${resolvedPath}`,
      status: EXIT.NOT_FOUND,
    };
  }

  const requests = entries
    .map(name => buildLocalSkillRequest(path.join(resolvedPath, name)))
    .filter((request): request is SkillInstallRequest => request !== null);

  if (requests.length > 0) {
    return { requests };
  }

  return {
    error: `Local path does not contain a SKILL.md or direct child skill folders: ${resolvedPath}`,
    status: EXIT.NOT_FOUND,
  };
}
