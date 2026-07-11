import { c, bold, dim } from '../../utils/colors.js';
import { selectWithCancel } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { clearScreen } from '../../utils/platform.js';
import { Spinner } from '../../utils/spinner.js';
import {
  checkAndPrintEnvironmentWithLoader,
  hasEnvironmentIssues,
} from '../install/index.js';
import { printGoodbye, printWelcome } from '../header.js';
import { runSkillsMenu } from '../skills-menu/index.js';
import { runOctocodeSkillsFlow } from '../skills-menu/marketplace.js';
import { getAppState, type AppState } from '../state.js';
import { runToolTerminalFlow } from '../tool-terminal.js';
import type { MenuChoice } from './types.js';
import {
  buildStatusLine,
  buildOctocodeMenuItem,
  buildOctocodeSkillsMenuItem,
  buildSkillsMenuItem,
  buildAuthMenuItem,
  printContextualHints,
} from './main-menu-items.js';
import { runOctocodeFlow } from './octocode-flow.js';
import { runAuthFlow } from './auth-flow.js';

async function showMainMenu(state: AppState): Promise<MenuChoice> {
  console.log();
  console.log(`  ${dim('Status:')} ${buildStatusLine(state)}`);

  printContextualHints(state);

  const choices: Array<{
    name: string;
    value: MenuChoice;
    description?: string;
  }> = [];

  choices.push(buildOctocodeMenuItem(state));

  choices.push(buildOctocodeSkillsMenuItem(state.skills));

  choices.push(buildSkillsMenuItem(state.skills));

  choices.push(buildAuthMenuItem(state.githubAuth));

  choices.push({
    name: '- Tool Terminal',
    value: 'terminal',
    description: 'Run Octocode tools directly from an interactive terminal',
  });

  choices.push(
    separatorChoice<{
      name: string;
      value: MenuChoice;
    }>()
  );
  choices.push({
    name: dim('Exit'),
    value: 'exit',
  });

  console.log();
  const choice = await selectWithCancel<MenuChoice>({
    message: 'What would you like to do?',
    choices,
    pageSize: 12,
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

async function handleMenuChoice(choice: MenuChoice): Promise<boolean> {
  switch (choice) {
    case 'octocode':
      await runOctocodeFlow();
      return true;

    case 'octocode-skills':
      await runOctocodeSkillsFlow();
      return true;

    case 'skills':
      await runSkillsMenu();
      return true;

    case 'auth':
      await runAuthFlow();
      return true;

    case 'terminal':
      await runToolTerminalFlow();
      return true;

    case 'exit':
      printGoodbye();
      return false;

    default:
      return true;
  }
}

function printEnvHeader(): void {
  console.log(`  ${bold('Environment')}`);
}

async function displayEnvironmentStatus(): Promise<void> {
  printEnvHeader();

  const envStatus = await checkAndPrintEnvironmentWithLoader();

  if (hasEnvironmentIssues(envStatus)) {
    console.log();
    console.log(
      `  ${dim('Tip:')} ${dim('Run')} ${c('cyan', 'npx node-doctor')} ${dim('for diagnostics')}`
    );
  }
}

export async function runMenuLoop(): Promise<void> {
  let firstRun = true;
  let running = true;

  while (running) {
    let state;
    if (firstRun) {
      state = await getAppState();
    } else {
      const spinner = new Spinner('  Loading...').start();
      state = await getAppState();
      spinner.clear();
    }

    if (!firstRun) {
      clearScreen();
      printWelcome();

      await displayEnvironmentStatus();
    }
    firstRun = false;

    const choice = await showMainMenu(state);
    running = await handleMenuChoice(choice);
  }
}
