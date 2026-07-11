import path from 'node:path';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import type { MarketplaceSource } from '../../../configs/skills-marketplace.js';
import { dirExists, fileExists } from '../../../utils/fs.js';
import {
  getAvailableSkills,
  getSkillsSourceDir,
  isSafeSkillName,
} from '../../../utils/skills.js';
import { formatSkillName } from './naming.js';
import type { SkillInstallRequest } from './types.js';

export function getCanonicalSkillSourceRoot(): string {
  return path.join(paths.home, 'skills');
}

export function getSkillSourcePath(skillName: string): string {
  return path.join(getCanonicalSkillSourceRoot(), skillName);
}

export function getBundledSkillsSource(): MarketplaceSource | null {
  try {
    const skillsPath = getSkillsSourceDir();
    return {
      id: 'bundled-octocode-skills',
      name: 'Bundled',
      type: 'local',
      owner: '',
      repo: '',
      branch: '',
      skillsPath,
      skillPattern: 'skill-folders',
      description: 'Bundled Octocode skills',
      url: `file://${skillsPath}`,
    };
  } catch {
    return null;
  }
}

export function tryResolveBundledSkillRequest(
  skillName: string
): SkillInstallRequest | null {
  try {
    if (!isSafeSkillName(skillName)) return null;

    const bundledSource = getBundledSkillsSource();
    if (!bundledSource) return null;

    const skillPath = path.join(bundledSource.skillsPath, skillName);
    if (
      !dirExists(skillPath) ||
      !fileExists(path.join(skillPath, 'SKILL.md'))
    ) {
      return null;
    }

    return {
      skill: {
        name: skillName,
        displayName: formatSkillName(skillName),
        description: 'Bundled Octocode skill',
        path: skillName,
        source: bundledSource,
      },
      sourceUrl: `file://${skillPath}`,
    };
  } catch {
    return null;
  }
}

export function resolveAllBundledSkillRequests(): SkillInstallRequest[] {
  try {
    const bundledSource = getBundledSkillsSource();
    if (!bundledSource) return [];

    return getAvailableSkills()
      .filter(name =>
        fileExists(path.join(bundledSource.skillsPath, name, 'SKILL.md'))
      )
      .map(name => ({
        skill: {
          name,
          displayName: formatSkillName(name),
          description: 'Bundled Octocode skill',
          path: name,
          source: bundledSource,
        },
        sourceUrl: `file://${path.join(bundledSource.skillsPath, name)}`,
      }));
  } catch {
    return [];
  }
}
