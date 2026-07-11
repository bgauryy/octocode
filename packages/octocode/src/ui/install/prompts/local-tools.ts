import { c, dim, bold } from '../../../utils/colors.js';
import { select } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import type { LocalToolsChoice } from './types.js';

export async function promptLocalTools(): Promise<boolean | null> {
  console.log();
  console.log(`  ${c('blue', 'INFO')} ${bold('Local Tools')}`);
  console.log(
    `  ${dim('Local filesystem tools search and read files in your local')}`
  );
  console.log(`  ${dim('codebase. They are enabled by default.')}`);
  console.log();

  const choice = await select<LocalToolsChoice>({
    message: 'Enable local tools?',
    choices: [
      {
        name: `${c('green', '●')} Enable ${dim('(Recommended)')} - ${dim('Allow local file exploration')}`,
        value: 'enable' as const,
      },
      {
        name: `${c('yellow', '○')} Disable - ${dim('Use only GitHub tools')}`,
        value: 'disable' as const,
      },
      separatorChoice<{ name: string; value: LocalToolsChoice }>(),
      {
        name: `${c('dim', '- Back')}`,
        value: 'back' as const,
      },
    ],
    loop: false,
  });

  if (choice === 'back') return null;
  return choice === 'enable';
}
