import { c, bold, dim } from '../../../utils/colors.js';
import { select, input } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import type {
  InstalledSkill,
  SkillsMenuChoice,
  ManageSkillsChoice,
  SkillActionChoice,
} from './types.js';

export async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

export async function showSkillsMenu(
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

export async function selectInstalledSkill(
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

export async function showSkillActions(
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
