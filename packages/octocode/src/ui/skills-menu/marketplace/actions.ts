import { c, bold, dim } from '../../../utils/colors.js';
import { select } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import { Spinner } from '../../../utils/spinner.js';
import { removeDirectory } from '../../../utils/fs.js';
import { getSkillsDestDir } from '../../../utils/skills.js';
import { type MarketplaceSkill } from '../../../configs/skills-marketplace.js';
import { installMarketplaceSkill } from '../../../utils/skills-fetch.js';
import path from 'node:path';
import {
  isSkillInstalled,
  RECOMMENDED_SKILLS,
  pressEnterToContinue,
  type InstallChoice,
} from './helpers.js';

export async function showSkillDetails(
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

export async function installSkill(skill: MarketplaceSkill): Promise<boolean> {
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

export async function deleteSkill(skill: MarketplaceSkill): Promise<boolean> {
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
