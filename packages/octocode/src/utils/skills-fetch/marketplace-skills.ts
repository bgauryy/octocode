import type {
  MarketplaceSource,
  MarketplaceSkill,
} from '../../configs/skills-marketplace.js';
import { isLocalSource } from '../../configs/skills-marketplace.js';
import { writeFileContent, dirExists } from '../fs.js';
import { join, isAbsolute } from 'node:path';
import { mkdirSync } from 'node:fs';
import { isPathInside, resolveSkillDestination } from '../skills.js';
import { parseSkillFrontmatter } from '../parsers/frontmatter.js';
import { readCachedSkills, writeCachedSkills } from './cache.js';
import { fetchMarketplaceTree, fetchRawContent } from './github-fetch.js';
import { fetchLocalSkills, installLocalSkill } from './local-skills.js';
import { formatSkillName, extractFirstParagraph } from './format.js';
import { prepareSkillDestination } from './skill-destination.js';
import { MAX_SKILL_FILES } from './constants.js';

export async function fetchMarketplaceSkills(
  source: MarketplaceSource,
  options: { skipCache?: boolean } = {}
): Promise<MarketplaceSkill[]> {
  if (isLocalSource(source)) {
    return fetchLocalSkills(source);
  }

  if (!options.skipCache) {
    const cached = readCachedSkills(source);
    if (cached) {
      return cached;
    }
  }

  const tree = await fetchMarketplaceTree(source);
  const skills: MarketplaceSkill[] = [];

  if (source.skillPattern === 'flat-md') {
    const prefix = source.skillsPath ? `${source.skillsPath}/` : '';
    const mdFiles = tree.filter(
      item =>
        item.type === 'blob' &&
        item.path.startsWith(prefix) &&
        item.path.endsWith('.md') &&
        !item.path.includes('/') === (prefix === '') &&
        (prefix === '' || item.path.split('/').length === 2)
    );

    const filesToFetch = mdFiles.slice(0, 100);

    const results = await Promise.all(
      filesToFetch.map(async file => {
        try {
          const content = await fetchRawContent(source, file.path);
          const meta = parseSkillFrontmatter(content);
          const filename = file.path.split('/').pop() || file.path;
          return {
            name: filename.replace(/\.md$/i, ''),
            displayName: formatSkillName(filename),
            description: meta?.description || 'No description available',
            category: meta?.category,
            path: file.path,
            source,
          } as MarketplaceSkill;
        } catch {
          return null;
        }
      })
    );
    skills.push(...results.filter((s): s is MarketplaceSkill => s !== null));
  } else {
    const prefix = source.skillsPath ? `${source.skillsPath}/` : '';

    const skillDirs = tree.filter(item => {
      if (
        item.type !== 'tree' ||
        (prefix !== '' && !item.path.startsWith(prefix))
      ) {
        return false;
      }

      const relativePath =
        prefix === '' ? item.path : item.path.slice(prefix.length);
      return (
        relativePath.length > 0 &&
        !relativePath.includes('/') &&
        !relativePath.includes('.') &&
        !relativePath.startsWith('.')
      );
    });

    const results = await Promise.all(
      skillDirs.slice(0, 50).map(async dir => {
        const skillMdPath = `${dir.path}/SKILL.md`;
        const readmePath = `${dir.path}/README.md`;

        const hasSkillMd = tree.some(
          item => item.path === skillMdPath && item.type === 'blob'
        );
        const hasReadme = tree.some(
          item => item.path === readmePath && item.type === 'blob'
        );

        const filePath = hasSkillMd
          ? skillMdPath
          : hasReadme
            ? readmePath
            : null;

        if (!filePath) return null;

        try {
          const content = await fetchRawContent(source, filePath);
          const meta = parseSkillFrontmatter(content);
          const folderName = dir.path.split('/').pop() || dir.path;
          return {
            name: folderName,
            displayName: formatSkillName(folderName),
            description:
              meta?.description ||
              extractFirstParagraph(content) ||
              'No description',
            category: meta?.category,
            path: dir.path,
            source,
          } as MarketplaceSkill;
        } catch {
          return null;
        }
      })
    );
    skills.push(...results.filter((s): s is MarketplaceSkill => s !== null));
  }

  writeCachedSkills(source, skills);

  return skills;
}

export async function installMarketplaceSkill(
  skill: MarketplaceSkill,
  destDir: string
): Promise<{ success: boolean; error?: string }> {
  try {
    const source = skill.source;

    if (isLocalSource(source)) {
      return installLocalSkill(skill, destDir);
    }

    const tree = await fetchMarketplaceTree(source);

    const skillDestDir = resolveSkillDestination(destDir, skill.name);
    if (!skillDestDir) {
      return { success: false, error: 'Invalid skill name' };
    }

    if (source.skillPattern === 'flat-md') {
      const content = await fetchRawContent(source, skill.path);
      const skillMdPath = join(skillDestDir, 'SKILL.md');
      if (!isPathInside(skillDestDir, skillMdPath)) {
        throw new Error('Invalid skill destination path');
      }
      prepareSkillDestination(skillDestDir);
      if (!writeFileContent(skillMdPath, content)) {
        throw new Error('Failed to write skill file');
      }
    } else {
      const prefix = skill.path ? `${skill.path}/` : '';
      const files = tree.filter(
        item => item.type === 'blob' && item.path.startsWith(prefix)
      );

      if (files.length === 0) {
        throw new Error(`Skill folder not found: ${skill.path || '/'}`);
      }

      if (files.length > MAX_SKILL_FILES) {
        throw new Error(
          `Skill has too many files: ${files.length} exceeds ${MAX_SKILL_FILES}`
        );
      }

      const plannedFiles = files.map(file => {
        const relativePath = file.path.slice(prefix.length);
        if (!relativePath || isAbsolute(relativePath)) {
          throw new Error('Invalid skill file path');
        }
        const destPath = join(skillDestDir, relativePath);
        if (!isPathInside(skillDestDir, destPath)) {
          throw new Error('Invalid skill file path traversal');
        }
        return { relativePath, destPath, sourcePath: file.path };
      });

      const fetchedFiles = await Promise.all(
        plannedFiles.map(async file => ({
          ...file,
          content: await fetchRawContent(source, file.sourcePath),
        }))
      );

      prepareSkillDestination(skillDestDir);

      for (const file of fetchedFiles) {
        const destSubDir = join(
          skillDestDir,
          file.relativePath.split('/').slice(0, -1).join('/')
        );
        if (destSubDir !== skillDestDir && !dirExists(destSubDir)) {
          mkdirSync(destSubDir, { recursive: true, mode: 0o700 });
        }
        if (!writeFileContent(file.destPath, file.content)) {
          throw new Error(`Failed to write skill file: ${file.relativePath}`);
        }
      }
    }

    return { success: true };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}
