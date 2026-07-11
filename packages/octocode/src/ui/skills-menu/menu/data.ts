import path from 'node:path';
import {
  dirExists,
  listSubdirectories,
  fileExists,
  readFileContent,
} from '../../../utils/fs.js';
import { getSkillsSourceDir, getSkillsDestDir } from '../../../utils/skills.js';
import { parseSkillFrontmatter } from '../../../utils/parsers/frontmatter.js';
import { RECOMMENDED_SKILLS, type InstalledSkill } from './types.js';

export function formatSkillName(name: string): string {
  const acronyms = ['PR', 'API', 'UI', 'CLI', 'MCP', 'AI'];
  const formatted = name
    .replace(/^octocode-/, '')
    .split('-')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');

  return formatted.replace(
    new RegExp(`\\b(${acronyms.join('|')})\\b`, 'gi'),
    match => match.toUpperCase()
  );
}

export function getAllInstalledSkills(): InstalledSkill[] {
  const destDir = getSkillsDestDir();
  const srcDir = getSkillsSourceDir();

  if (!dirExists(destDir)) {
    return [];
  }

  const skillFolders = listSubdirectories(destDir).filter(
    name => !name.startsWith('.')
  );

  const skills: InstalledSkill[] = [];

  for (const folder of skillFolders) {
    const skillPath = path.join(destDir, folder);
    const skillMdPath = path.join(skillPath, 'SKILL.md');

    const isBundled =
      folder.startsWith('octocode-') &&
      dirExists(srcDir) &&
      dirExists(path.join(srcDir, folder));

    const isRecommended = RECOMMENDED_SKILLS.has(folder);

    if (fileExists(skillMdPath)) {
      const content = readFileContent(skillMdPath);
      if (content) {
        const parsed = parseSkillFrontmatter(content);
        if (parsed?.name && parsed.description) {
          skills.push({
            name: parsed.name,
            description: parsed.description,
            folder,
            path: skillPath,
            isBundled,
            isRecommended,
          });
          continue;
        }
      }
    }

    skills.push({
      name: formatSkillName(folder),
      description: 'No description available',
      folder,
      path: skillPath,
      isBundled,
      isRecommended,
    });
  }

  skills.sort((a, b) => {
    if (a.isRecommended !== b.isRecommended) {
      return a.isRecommended ? -1 : 1;
    }
    return a.name.localeCompare(b.name);
  });

  return skills;
}

export function getSkillsInfo(): {
  srcDir: string;
  destDir: string;
  skillsStatus: Array<{
    name: string;
    installed: boolean;
    srcPath: string;
    destPath: string;
  }>;
  notInstalled: Array<{
    name: string;
    installed: boolean;
    srcPath: string;
    destPath: string;
  }>;
  sourceExists: boolean;
} {
  const srcDir = getSkillsSourceDir();
  const destDir = getSkillsDestDir();

  if (!dirExists(srcDir)) {
    return {
      srcDir,
      destDir,
      skillsStatus: [],
      notInstalled: [],
      sourceExists: false,
    };
  }

  const availableSkills = listSubdirectories(srcDir).filter(
    name => !name.startsWith('.')
  );

  const skillsStatus = availableSkills.map(skill => ({
    name: skill,
    installed: dirExists(path.join(destDir, skill)),
    srcPath: path.join(srcDir, skill),
    destPath: path.join(destDir, skill),
  }));

  const notInstalled = skillsStatus.filter(s => !s.installed);

  return { srcDir, destDir, skillsStatus, notInstalled, sourceExists: true };
}
