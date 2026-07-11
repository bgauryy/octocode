import type {
  MarketplaceSource,
  MarketplaceSkill,
} from '../../configs/skills-marketplace.js';
import { dirExists, readFileContent, fileExists } from '../fs.js';
import { join, isAbsolute, resolve } from 'node:path';
import { copyFileSync, mkdirSync, statSync, readdirSync } from 'node:fs';
import {
  getSkillsSourcePath,
  getAvailableSkills,
  isSafeSkillName,
  isPathInside,
  resolveSkillDestination,
} from '../skills.js';
import { parseSkillFrontmatter } from '../parsers/frontmatter.js';
import {
  MAX_CONTENT_SIZE_BYTES,
  MAX_SKILL_FILES,
  LOCAL_SKILL_IGNORED_DIRS,
  LOCAL_SKILL_IGNORED_FILES,
} from './constants.js';
import { formatSkillName, extractFirstParagraph } from './format.js';
import { prepareSkillDestination } from './skill-destination.js';

export function fetchLocalSkills(
  source: MarketplaceSource
): MarketplaceSkill[] {
  try {
    const skillsSourcePath = getLocalSkillsSourcePath(source);
    const availableSkills = isAbsolute(source.skillsPath)
      ? listLocalSkillFolders(skillsSourcePath)
      : getAvailableSkills();
    const skills: MarketplaceSkill[] = [];

    for (const skillFolder of availableSkills) {
      const skillPath = join(skillsSourcePath, skillFolder);
      const skillMdPath = join(skillPath, 'SKILL.md');

      if (fileExists(skillMdPath)) {
        const content = readFileContent(skillMdPath);
        if (content) {
          const meta = parseSkillFrontmatter(content);
          skills.push({
            name: skillFolder,
            displayName: formatSkillName(skillFolder),
            description:
              meta?.description ||
              extractFirstParagraph(content) ||
              'No description',
            category: meta?.category || 'Official',
            path: skillFolder,
            source,
          });
        }
      }
    }

    return skills;
  } catch {
    return [];
  }
}

export function installLocalSkill(
  skill: MarketplaceSkill,
  destDir: string
): { success: boolean; error?: string } {
  try {
    const skillsSourcePath = getLocalSkillsSourcePath(skill.source);
    const sourcePath = resolveSkillDestination(
      skillsSourcePath,
      skill.path || skill.name
    );
    const destPath = resolveSkillDestination(destDir, skill.name);

    if (!sourcePath || !destPath) {
      return { success: false, error: 'Invalid skill name' };
    }

    if (!dirExists(sourcePath)) {
      return { success: false, error: 'Skill not found in local source' };
    }

    if (!fileExists(join(sourcePath, 'SKILL.md'))) {
      return { success: false, error: 'Local skill is missing SKILL.md' };
    }

    if (resolve(sourcePath) === resolve(destPath)) {
      return { success: true };
    }

    copyLocalSkillDirectory(sourcePath, destPath);

    return { success: true };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

function isIgnoredLocalSkillPath(relativePath: string): boolean {
  const parts = relativePath.split(/[\\/]+/).filter(Boolean);
  const name = parts.at(-1) ?? '';

  return (
    parts.some(part => part.startsWith('.')) ||
    parts.some(part => LOCAL_SKILL_IGNORED_DIRS.has(part)) ||
    LOCAL_SKILL_IGNORED_FILES.has(name)
  );
}

function planLocalSkillFiles(
  sourcePath: string,
  destPath: string
): { sourcePath: string; destPath: string; relativePath: string }[] {
  const files: {
    sourcePath: string;
    destPath: string;
    relativePath: string;
  }[] = [];
  const queue = [{ sourceDir: sourcePath, relativeDir: '' }];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) break;

    for (const entry of readdirSync(current.sourceDir, {
      withFileTypes: true,
    })) {
      const relativePath = current.relativeDir
        ? join(current.relativeDir, entry.name)
        : entry.name;

      if (entry.isSymbolicLink() || isIgnoredLocalSkillPath(relativePath)) {
        continue;
      }

      const entrySourcePath = join(current.sourceDir, entry.name);

      if (entry.isDirectory()) {
        queue.push({ sourceDir: entrySourcePath, relativeDir: relativePath });
        continue;
      }

      if (!entry.isFile()) {
        continue;
      }

      const entryDestPath = join(destPath, relativePath);
      if (!isPathInside(destPath, entryDestPath)) {
        throw new Error('Invalid local skill file path traversal');
      }

      const sizeBytes = statSync(entrySourcePath).size;
      if (sizeBytes > MAX_CONTENT_SIZE_BYTES) {
        throw new Error(
          `Local skill file too large: ${relativePath} exceeds ${MAX_CONTENT_SIZE_BYTES} byte limit`
        );
      }

      files.push({
        sourcePath: entrySourcePath,
        destPath: entryDestPath,
        relativePath,
      });

      if (files.length > MAX_SKILL_FILES) {
        throw new Error(
          `Local skill has too many files: ${files.length} exceeds ${MAX_SKILL_FILES}`
        );
      }
    }
  }

  if (!files.some(file => file.relativePath === 'SKILL.md')) {
    throw new Error('Local skill is missing SKILL.md');
  }

  return files;
}

function copyLocalSkillDirectory(sourcePath: string, destPath: string): void {
  const plannedFiles = planLocalSkillFiles(sourcePath, destPath);
  prepareSkillDestination(destPath);

  for (const file of plannedFiles) {
    const destSubDir = join(
      destPath,
      file.relativePath
        .split(/[\\/]+/)
        .slice(0, -1)
        .join('/')
    );
    if (destSubDir !== destPath && !dirExists(destSubDir)) {
      mkdirSync(destSubDir, { recursive: true, mode: 0o700 });
    }
    copyFileSync(file.sourcePath, file.destPath);
  }
}

function getLocalSkillsSourcePath(source: MarketplaceSource): string {
  return source.skillsPath && isAbsolute(source.skillsPath)
    ? source.skillsPath
    : getSkillsSourcePath();
}

function listLocalSkillFolders(skillsSourcePath: string): string[] {
  try {
    if (!dirExists(skillsSourcePath)) {
      return [];
    }
    return readdirSync(skillsSourcePath, { withFileTypes: true })
      .filter(entry => entry.isDirectory() || entry.isSymbolicLink())
      .map(entry => entry.name)
      .filter(isSafeSkillName)
      .filter(name => fileExists(join(skillsSourcePath, name, 'SKILL.md')));
  } catch {
    return [];
  }
}
