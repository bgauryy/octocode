import { c, bold } from '../../utils/colors.js';
import { loadInquirer, selectWithCancel } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { Spinner } from '../../utils/spinner.js';
import { runInstallFlow } from '../install/index.js';
import { runConfigOptionsFlow } from '../config/index.js';
import { getAppState, type AppState } from '../state.js';
import type { OctocodeMenuChoice } from './types.js';
import { getClientNames, printInstalledIDEs } from './main-menu-items.js';

async function showOctocodeMenu(state: AppState): Promise<OctocodeMenuChoice> {
  const choices: Array<{
    name: string;
    value: OctocodeMenuChoice;
    description?: string;
  }> = [];

  if (state.octocode.isInstalled) {
    if (state.octocode.hasMoreToInstall) {
      const availableNames = getClientNames(state.octocode.availableClients);
      choices.push({
        name: '- Add Octocode',
        value: 'install',
        description: availableNames,
      });
    }
  } else {
    choices.push({
      name: `- ${bold('Install')} ${c('red', '[X]')}`,
      value: 'install',
      description: 'Setup for Cursor, Claude, Windsurf...',
    });
  }

  if (state.octocode.isInstalled) {
    choices.push({
      name: '- Configure Octocode',
      value: 'configure',
      description: 'Server options & preferences',
    });
  }

  choices.push(
    separatorChoice<{
      name: string;
      value: OctocodeMenuChoice;
      description?: string;
    }>()
  );

  choices.push({
    name: `${c('dim', '- Back to main menu')}`,
    value: 'back',
  });

  const choice = await selectWithCancel<OctocodeMenuChoice>({
    message: '',
    choices,
    pageSize: 12,
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

export async function runOctocodeFlow(): Promise<void> {
  await loadInquirer();

  let state = await getAppState();

  console.log();
  printInstalledIDEs(state.octocode.installedClients);

  let inMenu = true;
  let firstRun = true;
  while (inMenu) {
    if (firstRun) {
      firstRun = false;
    } else {
      const spinner = new Spinner('  Refreshing...').start();
      state = await getAppState();
      spinner.clear();
    }

    const choice = await showOctocodeMenu(state);

    switch (choice) {
      case 'install':
        await runInstallFlow();
        console.log();
        break;

      case 'configure':
        await runConfigOptionsFlow();
        console.log();
        break;

      case 'back':
      default:
        inMenu = false;
        break;
    }
  }
}
