import { c } from '../../utils/colors.js';
import { loadInquirer, select } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { runEditConfigFlow } from './edit-flow.js';
import {
  pressEnterToContinue,
  showConfigInfo,
  showCurrentJsonConfig,
} from './view.js';

type ConfigMenuChoice = 'edit' | 'view' | 'show-json' | 'back';

async function showConfigMenu(): Promise<ConfigMenuChoice> {
  const choice = await select<ConfigMenuChoice>({
    message: '',
    choices: [
      {
        name: '- Edit configuration',
        value: 'edit',
        description: 'Configure all @octocodeai/mcp settings for a client',
      },
      {
        name: '- View all configuration options',
        value: 'view',
        description: 'Show available environment variables and their defaults',
      },
      {
        name: '- Show current JSON config',
        value: 'show-json',
        description: 'Display the actual MCP config JSON for a client',
      },
      separatorChoice<{ name: string; value: ConfigMenuChoice }>(),
      {
        name: `${c('dim', '- Back to main menu')}`,
        value: 'back',
      },
    ],
    pageSize: 10,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
      },
    },
  });

  return choice;
}

export async function runConfigOptionsFlow(): Promise<void> {
  await loadInquirer();

  const choice = await showConfigMenu();

  switch (choice) {
    case 'view':
      showConfigInfo();

      await pressEnterToContinue();
      break;

    case 'edit':
      await runEditConfigFlow();
      break;

    case 'show-json':
      await showCurrentJsonConfig();
      break;

    case 'back':
    default:
      break;
  }
}
