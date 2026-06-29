import { c, bold, dim } from '../../utils/colors.js';
import { loadInquirer, select, input } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import {
  dirExists,
  listSubdirectories,
  removeDirectory,
  fileExists,
  readFileContent,
} from '../../utils/fs.js';
import { getSkillsSourceDir, getSkillsDestDir } from '../../utils/skills.js';
import { parseSkillFrontmatter } from '../../utils/parsers/frontmatter.js';
import path from 'node:path';
import open from 'open';
import { Spinner } from '../../utils/spinner.js';
import { runMarketplaceFlow } from './marketplace.js';

const RECOMMENDED_SKILLS = new Set(['octocode-research']);

interface InstalledSkill {
  name: string;

  description: string;

  folder: string;

  path: string;

  isBundled: boolean;

  isRecommended: boolean;
}

type SkillsMenuChoice = 'manage' | 'marketplace' | 'back';
type ManageSkillsChoice = InstalledSkill | 'back';

async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

function getAllInstalledSkills(): InstalledSkill[] {
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

function getSkillsInfo(): {
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

async function showSkillsMenu(
  installedCount: number
): Promise<SkillsMenuChoice> {
  const choices: Array<{
    name: string;
    value: SkillsMenuChoice;
    description?: string;
  }> = [];

  if (installedCount > 0) {
    choices.push({
      name: `- Manage installed skills ${dim(`(${installedCount})`)}`,
      value: 'manage',
      description: 'View, remove, or inspect individual skills',
    });
  }

  choices.push({
    name: '- Browse Marketplace',
    value: 'marketplace',
    description: 'Community skills • installs on your behalf',
  });
  choices.push(
    separatorChoice<{
      name: string;
      value: SkillsMenuChoice;
      description?: string;
    }>()
  );
  choices.push({
    name: `${c('dim', '- Back to main menu')}`,
    value: 'back',
  });

  const choice = await select<SkillsMenuChoice>({
    message: '',
    choices,
    pageSize: 10,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}

function formatSkillName(name: string): string {
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

async function selectInstalledSkill(
  skills: InstalledSkill[]
): Promise<ManageSkillsChoice> {
  console.log();
  console.log(
    `  ${bold('Installed Skills')} ${dim(`(${skills.length} total)`)}`
  );
  console.log(`  ${dim('Select a skill to manage')}`);
  console.log();

  const choices: Array<{
    name: string;
    value: ManageSkillsChoice;
  }> = [];

  for (const skill of skills) {
    const starTag = skill.isRecommended ? c('yellow', '★ ') : '';
    const sourceTag = skill.isBundled
      ? c('cyan', ' [bundled]')
      : c('magenta', ' [community]');
    const desc = skill.description.slice(0, 40);
    const ellipsis = skill.description.length > 40 ? '...' : '';

    choices.push({
      name: `${starTag}${skill.name}${sourceTag} - ${dim(desc)}${dim(ellipsis)}`,
      value: skill,
    });
  }

  choices.push(separatorChoice<{ name: string; value: ManageSkillsChoice }>());
  choices.push({
    name: `${c('dim', '- Back to skills menu')}`,
    value: 'back',
  });

  const choice = await select<ManageSkillsChoice>({
    message: '',
    choices,
    pageSize: 15,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}

type SkillActionChoice = 'remove' | 'view' | 'back';

async function showSkillActions(
  skill: InstalledSkill
): Promise<SkillActionChoice> {
  const recommendedTag = skill.isRecommended ? c('yellow', 'recommended ') : '';
  const sourceTag = skill.isBundled
    ? c('cyan', '[bundled]')
    : c('magenta', '[community]');

  console.log();
  console.log(`  ${bold(skill.name)} ${recommendedTag}${sourceTag}`);
  console.log(`  ${dim(skill.description)}`);
  console.log(`  ${dim(skill.path)}`);
  console.log();

  const choices: Array<{ name: string; value: SkillActionChoice }> = [
    {
      name: `${c('red', 'Delete')} Remove this skill`,
      value: 'remove',
    },
    {
      name: `- Open skill location`,
      value: 'view',
    },
    separatorChoice<{ name: string; value: SkillActionChoice }>(),
    {
      name: `${c('dim', '- Back')}`,
      value: 'back',
    },
  ];

  const choice = await select<SkillActionChoice>({
    message: '',
    choices,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}

async function openSkillLocation(skill: InstalledSkill): Promise<void> {
  console.log();
  console.log(`  ${c('cyan', 'Path')} Opening ${bold(skill.name)} location...`);
  console.log(`  ${dim(skill.path)}`);
  console.log();

  try {
    await open(skill.path);
    console.log(`  ${c('green', '✅')} Opened in file explorer`);
  } catch {
    console.log(`  ${c('yellow', '!')} Could not open location automatically`);
    console.log(`  ${dim('Path:')} ${c('cyan', skill.path)}`);
  }
  console.log();
}

async function removeSkill(skill: InstalledSkill): Promise<boolean> {
  console.log();
  console.log(`  ${c('yellow', 'WARN')} You are about to remove:`);
  console.log(`    ${bold(skill.name)}`);
  console.log(`    ${dim(skill.path)}`);
  console.log();

  const choices = [
    {
      name: `${c('red', 'Delete')} Yes, remove this skill`,
      value: true,
    },
    separatorChoice<{ name: string; value: boolean }>(),
    {
      name: `${c('dim', '- Cancel')}`,
      value: false,
    },
  ];

  const confirmed = await select<boolean>({
    message: 'Confirm removal?',
    choices,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
        message: (text: string) => bold(text),
      },
    },
  });

  if (!confirmed) {
    return false;
  }

  console.log();
  const spinner = new Spinner(`Removing ${skill.name}...`).start();

  if (removeDirectory(skill.path)) {
    spinner.succeed(`Removed ${skill.name}`);
    console.log();
    console.log(`  ${c('green', '✅')} Skill removed successfully`);
    return true;
  } else {
    spinner.fail(`Failed to remove ${skill.name}`);
    console.log();
    console.log(`  ${c('red', 'X')} Could not remove skill directory`);
    return false;
  }
}

async function manageInstalledSkills(): Promise<void> {
  let inManageMenu = true;

  while (inManageMenu) {
    const installedSkills = getAllInstalledSkills();

    if (installedSkills.length === 0) {
      console.log();
      console.log(`  ${c('yellow', 'INFO')} No skills installed`);
      console.log(`  ${dim('Browse the marketplace to install skills')}`);
      console.log();
      await pressEnterToContinue();
      return;
    }

    const selectedSkill = await selectInstalledSkill(installedSkills);

    if (selectedSkill === 'back') {
      inManageMenu = false;
      continue;
    }

    let inSkillActions = true;
    while (inSkillActions) {
      const action = await showSkillActions(selectedSkill);

      switch (action) {
        case 'remove': {
          const removed = await removeSkill(selectedSkill);
          if (removed) {
            await pressEnterToContinue();
            inSkillActions = false;
          }
          break;
        }

        case 'view':
          await openSkillLocation(selectedSkill);
          await pressEnterToContinue();
          break;

        case 'back':
        default:
          inSkillActions = false;
          break;
      }
    }
  }
}

export async function runSkillsMenu(): Promise<void> {
  await loadInquirer();

  let info = getSkillsInfo();

  if (!info.sourceExists) {
    console.log(`  ${c('yellow', 'WARN')} Skills source directory not found.`);
    console.log(`  ${dim('This may happen if running from source.')}`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  if (info.skillsStatus.length === 0) {
    console.log(`  ${dim('No skills available.')}`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  let inSkillsMenu = true;
  while (inSkillsMenu) {
    info = getSkillsInfo();

    const installedSkills = getAllInstalledSkills();
    const installedCount = installedSkills.length;

    const choice = await showSkillsMenu(installedCount);

    switch (choice) {
      case 'manage':
        await manageInstalledSkills();
        break;

      case 'marketplace':
        await runMarketplaceFlow();
        break;

      case 'back':
      default:
        inSkillsMenu = false;
        break;
    }
  }
}
