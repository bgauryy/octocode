import { c, bold, dim } from '../../utils/colors.js';
import { select, input, search } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { Spinner } from '../../utils/spinner.js';
import { dirExists, removeDirectory } from '../../utils/fs.js';
import { getSkillsDestDir } from '../../utils/skills.js';
import {
  SKILLS_MARKETPLACES,
  type MarketplaceSource,
  type MarketplaceSkill,
  fetchAllMarketplaceStars,
  isLocalSource,
} from '../../configs/skills-marketplace.js';
import {
  fetchMarketplaceSkills,
  installMarketplaceSkill,
} from '../../utils/skills-fetch.js';
import path from 'node:path';

type MarketplaceMenuChoice = MarketplaceSource | 'back';
type SkillMenuChoice = MarketplaceSkill | 'back';
type InstallChoice = 'install' | 'delete' | 'back';
type OfficialFlowChoice = 'install-all' | 'browse' | 'back';

const RECOMMENDED_SKILLS = new Set([
  'octocode-research',
  'octocode-engineer',
  'octocode-engineer',
]);

async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

function formatMarketplace(source: MarketplaceSource, stars?: number): string {
  if (isLocalSource(source)) {
    return `${bold(source.name)} ${c('cyan', 'bundled')} - ${dim(source.description)}`;
  }
  const starsText = stars ? ` ${stars.toLocaleString()}` : '';
  return `${bold(source.name)}${c('yellow', starsText)} - ${dim(source.description)}`;
}

function formatSkill(skill: MarketplaceSkill, installed: boolean): string {
  const installedTag = installed ? c('green', '✅ ') : '';
  const starTag = RECOMMENDED_SKILLS.has(skill.name) ? c('yellow', ' *') : '';
  const desc = skill.description.slice(0, 50);
  const ellipsis = skill.description.length > 50 ? '...' : '';
  return `${installedTag}${skill.displayName}${starTag} ${dim(desc)}${dim(ellipsis)}`;
}

function isSkillInstalled(skillName: string): boolean {
  const destDir = getSkillsDestDir();
  return dirExists(path.join(destDir, skillName));
}

async function selectMarketplace(
  starsMap: Map<string, number>
): Promise<MarketplaceMenuChoice> {
  console.log();
  console.log(`  ${bold('Select a marketplace to browse:')}`);
  console.log();

  const localSources = SKILLS_MARKETPLACES.filter(s => isLocalSource(s));
  const githubSources = SKILLS_MARKETPLACES.filter(s => !isLocalSource(s));

  const sortedGitHubSources = [...githubSources].sort(
    (a, b) => (starsMap.get(b.id) ?? 0) - (starsMap.get(a.id) ?? 0)
  );

  const sortedMarketplaces = [...localSources, ...sortedGitHubSources];

  const choices: Array<{
    name: string;
    value: MarketplaceMenuChoice;
    description?: string;
  }> = [];

  for (const source of sortedMarketplaces) {
    choices.push({
      name: formatMarketplace(source, starsMap.get(source.id)),
      value: source,
      description: dim(source.url),
    });
  }

  choices.push(
    separatorChoice<{
      name: string;
      value: MarketplaceMenuChoice;
    }>()
  );
  choices.push({
    name: `${c('dim', '- Back to skills menu')}`,
    value: 'back',
  });

  const choice = await select<MarketplaceMenuChoice>({
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

async function browseSkills(
  source: MarketplaceSource,
  skills: MarketplaceSkill[]
): Promise<SkillMenuChoice> {
  console.log();
  console.log(`  ${bold(source.name)} - ${skills.length} skills available`);
  console.log(`  ${dim(source.url)}`);
  console.log();

  const sortedSkills = [...skills].sort((a, b) => {
    const aInstalled = isSkillInstalled(a.name);
    const bInstalled = isSkillInstalled(b.name);
    if (aInstalled !== bInstalled) {
      return aInstalled ? -1 : 1;
    }

    const aRecommended = RECOMMENDED_SKILLS.has(a.name);
    const bRecommended = RECOMMENDED_SKILLS.has(b.name);
    if (aRecommended !== bRecommended) {
      return aRecommended ? -1 : 1;
    }

    return a.displayName.localeCompare(b.displayName);
  });

  const skillChoices = sortedSkills.map(skill => {
    const installed = isSkillInstalled(skill.name);
    return {
      name: formatSkill(skill, installed),
      value: skill as SkillMenuChoice,

      description: skill.category ? `[${skill.category}]` : undefined,
    };
  });

  const backChoice = {
    name: `${c('dim', '- Back to marketplaces')}`,
    value: 'back' as SkillMenuChoice,
  };

  const choice = await search<SkillMenuChoice>({
    message: `Type to filter skills (${skills.length} available)`,
    source: (term: string | undefined) => {
      if (!term || !term.trim()) {
        return [...skillChoices, backChoice];
      }

      const lowerTerm = term.toLowerCase();
      const filtered = skillChoices.filter(choice => {
        if (typeof choice.value === 'string') return false;
        const skill = choice.value;
        return (
          skill.name.toLowerCase().includes(lowerTerm) ||
          skill.displayName.toLowerCase().includes(lowerTerm) ||
          skill.description.toLowerCase().includes(lowerTerm) ||
          skill.category?.toLowerCase().includes(lowerTerm)
        );
      });

      return [...filtered, backChoice];
    },
    pageSize: 20,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}

async function showSkillDetails(
  skill: MarketplaceSkill
): Promise<InstallChoice> {
  const installed = isSkillInstalled(skill.name);
  const destDir = getSkillsDestDir();
  const recommendedTag = RECOMMENDED_SKILLS.has(skill.name)
    ? c('yellow', ' recommended')
    : '';

  console.log();
  console.log(`  ${bold(skill.displayName)}${recommendedTag}`);
  console.log(`  ${dim(skill.description)}`);
  console.log();

  if (skill.category) {
    console.log(`  ${bold('Category:')} ${skill.category}`);
    console.log();
  }

  console.log(`  ${bold('Source:')} ${skill.source.name}`);
  console.log(`  ${dim(skill.source.url)}`);
  console.log();

  console.log(`  ${bold('Install path:')}`);
  console.log(`  ${c('cyan', path.join(destDir, skill.name))}`);
  console.log();

  const choices: Array<{
    name: string;
    value: InstallChoice;
  }> = [
    {
      name: installed
        ? `${c('yellow', '⬆')} Reinstall skill`
        : `${c('green', '✅')} Install skill`,
      value: 'install',
    },
  ];

  if (installed) {
    choices.push({
      name: `${c('red', 'Delete')} Delete skill`,
      value: 'delete',
    });
  }

  choices.push(separatorChoice<{ name: string; value: InstallChoice }>());
  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const choice = await select<InstallChoice>({
    message: installed ? 'Choose an action:' : 'Install this skill?',
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

  return choice;
}

async function installSkill(skill: MarketplaceSkill): Promise<boolean> {
  const destDir = getSkillsDestDir();

  console.log();
  const spinner = new Spinner(`Installing ${skill.displayName}...`).start();

  const result = await installMarketplaceSkill(skill, destDir);

  if (result.success) {
    spinner.succeed(`Installed ${skill.displayName}!`);
    console.log();
    console.log(`  ${c('green', '✅')} Skill installed successfully`);
    console.log(
      `  ${dim('Location:')} ${c('cyan', path.join(destDir, skill.name))}`
    );
    console.log();
    console.log(`  ${bold('The skill is now available in your AI client!')}`);
  } else {
    spinner.fail(`Failed to install ${skill.displayName}`);
    console.log();
    console.log(`  ${c('red', 'X')} Installation failed: ${result.error}`);
  }

  console.log();
  await pressEnterToContinue();
  return result.success;
}

async function deleteSkill(skill: MarketplaceSkill): Promise<boolean> {
  const destDir = getSkillsDestDir();
  const skillPath = path.join(destDir, skill.name);

  console.log();
  const confirm = await select<'yes' | 'no'>({
    message: 'Delete this installed skill?',
    choices: [
      { name: `${c('red', 'Delete')} Yes, delete skill`, value: 'yes' },
      separatorChoice<{ name: string; value: 'yes' | 'no' }>(),
      { name: `${c('dim', '- Cancel')}`, value: 'no' },
    ],
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
        message: (text: string) => bold(text),
      },
    },
  });

  if (confirm !== 'yes') {
    return false;
  }

  console.log();
  const spinner = new Spinner(`Deleting ${skill.displayName}...`).start();

  if (removeDirectory(skillPath)) {
    spinner.succeed(`Deleted ${skill.displayName}`);
    console.log();
    console.log(`  ${c('green', '✅')} Skill deleted successfully`);
    console.log();
    await pressEnterToContinue();
    return true;
  }

  spinner.fail(`Failed to delete ${skill.displayName}`);
  console.log();
  console.log(`  ${c('red', 'X')} Could not delete skill directory`);
  console.log(`  ${dim('Path:')} ${c('cyan', skillPath)}`);
  console.log();
  await pressEnterToContinue();
  return false;
}

async function showOfficialFlowMenu(
  totalSkills: number,
  notInstalledCount: number
): Promise<OfficialFlowChoice> {
  console.log();
  console.log(`  ${bold('Octocode Skills')}`);
  console.log(`  ${dim(`${totalSkills} skills available`)}`);
  console.log();

  const choices: Array<{
    name: string;
    value: OfficialFlowChoice;
    description?: string;
  }> = [];

  if (notInstalledCount > 0) {
    choices.push({
      name: `${c('green', 'Fast')} Install All Skills (${notInstalledCount} to install)`,
      value: 'install-all',
      description: dim('One-click install of all Octocode skills'),
    });
    choices.push({
      name: `${c('cyan', 'List')} Browse Skills Individually`,
      value: 'browse',
      description: dim('View details and install one by one'),
    });
  } else {
    choices.push({
      name: `${c('green', '✅')} All skills installed — Browse to reinstall or view details`,
      value: 'browse',
      description: dim('View details and reinstall individually'),
    });
  }

  choices.push(
    separatorChoice<{
      name: string;
      value: OfficialFlowChoice;
    }>()
  );
  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const choice = await select<OfficialFlowChoice>({
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

async function installAllSkills(skills: MarketplaceSkill[]): Promise<void> {
  const destDir = getSkillsDestDir();

  const skillsToInstall = skills.filter(skill => !isSkillInstalled(skill.name));

  if (skillsToInstall.length === 0) {
    console.log();
    console.log(`  ${c('green', '✅')} All skills are already installed!`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  console.log();
  console.log(
    `  ${bold('Installing')} ${skillsToInstall.length} ${bold('skills...')}`
  );
  console.log();

  const spinner = new Spinner(
    `Installing ${skillsToInstall.length} skills...`
  ).start();

  let installed = 0;
  let failed = 0;
  const errors: Array<{ skill: string; error: string }> = [];

  for (const skill of skillsToInstall) {
    spinner.update(
      `Installing ${skill.displayName}... (${installed + failed + 1}/${skillsToInstall.length})`
    );

    const result = await installMarketplaceSkill(skill, destDir);
    if (result.success) {
      installed++;
    } else {
      failed++;
      errors.push({
        skill: skill.displayName,
        error: result.error || 'Unknown error',
      });
    }
  }

  if (failed === 0) {
    spinner.succeed(`All ${installed} skills installed successfully!`);
  } else {
    spinner.warn(`Installed ${installed} skills, ${failed} failed`);
  }

  console.log();

  if (installed > 0) {
    console.log(
      `  ${c('green', '✅')} Successfully installed ${installed} skill(s)`
    );
    console.log(`  ${dim('Location:')} ${c('cyan', destDir)}`);
  }

  if (errors.length > 0) {
    console.log();
    console.log(`  ${c('red', 'X')} Failed to install:`);
    for (const { skill, error } of errors) {
      console.log(`    ${c('red', '•')} ${skill}: ${dim(error)}`);
    }
  }

  console.log();
  console.log(`  ${bold('Skills are now available in your AI client!')}`);
  console.log();

  await pressEnterToContinue();
}

export async function runMarketplaceFlow(): Promise<void> {
  console.log();
  console.log(
    `  ${c('yellow', 'WARN')} ${dim('Community list • Skills install on your behalf')}`
  );

  const starsSpinner = new Spinner('Fetching marketplace info...').start();
  let starsMap: Map<string, number>;
  try {
    starsMap = await fetchAllMarketplaceStars();
    starsSpinner.succeed('Loaded marketplace info');
  } catch {
    starsSpinner.fail('Could not fetch stars');
    starsMap = new Map();
  }

  let inMarketplace = true;

  while (inMarketplace) {
    const source = await selectMarketplace(starsMap);

    if (source === 'back') {
      inMarketplace = false;
      continue;
    }

    console.log();
    const spinner = new Spinner(
      `Loading skills from ${source.name}...`
    ).start();

    let skills: MarketplaceSkill[];
    try {
      skills = await fetchMarketplaceSkills(source);
      spinner.stop();
    } catch (error) {
      spinner.fail(`Failed to load skills`);
      console.log();
      console.log(
        `  ${c('red', 'X')} ${error instanceof Error ? error.message : 'Unknown error'}`
      );
      console.log();
      await pressEnterToContinue();
      continue;
    }

    if (skills.length === 0) {
      console.log();
      console.log(
        `  ${c('yellow', 'WARN')} No skills found in this marketplace`
      );
      console.log();
      await pressEnterToContinue();
      continue;
    }

    let inSkillsBrowser = true;
    while (inSkillsBrowser) {
      const skillChoice = await browseSkills(source, skills);

      if (skillChoice === 'back') {
        inSkillsBrowser = false;
        continue;
      }

      const detailChoice = await showSkillDetails(skillChoice);
      if (detailChoice === 'install') {
        await installSkill(skillChoice);
      } else if (detailChoice === 'delete') {
        await deleteSkill(skillChoice);
      }
    }
  }
}

export async function runOctocodeSkillsFlow(): Promise<void> {
  const source = SKILLS_MARKETPLACES.find(s => s.id === 'octocode-skills');
  if (!source) {
    console.log();
    console.log(`  ${c('red', 'X')} Octocode Skills source not found`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  console.log();
  const spinner = new Spinner(`Loading Octocode Skills...`).start();

  let skills: MarketplaceSkill[];
  try {
    skills = await fetchMarketplaceSkills(source);
    spinner.stop();
  } catch (error) {
    spinner.fail(`Failed to load skills`);
    console.log();
    console.log(
      `  ${c('red', 'X')} ${error instanceof Error ? error.message : 'Unknown error'}`
    );
    console.log();
    await pressEnterToContinue();
    return;
  }

  if (skills.length === 0) {
    console.log();
    console.log(`  ${c('yellow', 'WARN')} No skills found`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  let inFlow = true;
  while (inFlow) {
    const notInstalledCount = skills.filter(
      s => !isSkillInstalled(s.name)
    ).length;

    const menuChoice = await showOfficialFlowMenu(
      skills.length,
      notInstalledCount
    );

    switch (menuChoice) {
      case 'install-all':
        await installAllSkills(skills);
        inFlow = false;
        break;

      case 'browse': {
        let inSkillsBrowser = true;
        while (inSkillsBrowser) {
          const skillChoice = await browseSkills(source, skills);
          if (skillChoice === 'back') {
            inSkillsBrowser = false;
            continue;
          }
          const detailChoice = await showSkillDetails(skillChoice);
          if (detailChoice === 'install') {
            await installSkill(skillChoice);
          } else if (detailChoice === 'delete') {
            await deleteSkill(skillChoice);
          }
        }
        break;
      }

      case 'back':
      default:
        inFlow = false;
        break;
    }
  }
}
