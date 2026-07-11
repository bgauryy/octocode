import { c, bold, dim } from '../../../utils/colors.js';
import { select } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import { removeDirectory } from '../../../utils/fs.js';
import { Spinner } from '../../../utils/spinner.js';
import open from 'open';
import type { InstalledSkill } from './types.js';

export async function openSkillLocation(skill: InstalledSkill): Promise<void> {
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

export async function removeSkill(skill: InstalledSkill): Promise<boolean> {
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
