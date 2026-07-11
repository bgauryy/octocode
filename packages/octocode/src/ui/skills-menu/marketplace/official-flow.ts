import { c, bold, dim } from '../../../utils/colors.js';
import { select } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import { Spinner } from '../../../utils/spinner.js';
import { getSkillsDestDir } from '../../../utils/skills.js';
import { type MarketplaceSkill } from '../../../configs/skills-marketplace.js';
import { installMarketplaceSkill } from '../../../utils/skills-fetch.js';
import {
  isSkillInstalled,
  pressEnterToContinue,
  type OfficialFlowChoice,
} from './helpers.js';

export async function showOfficialFlowMenu(
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

export async function installAllSkills(
  skills: MarketplaceSkill[]
): Promise<void> {
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
