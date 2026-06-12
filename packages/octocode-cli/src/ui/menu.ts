import { c, bold, dim } from '../utils/colors.js';
import {
  loadInquirer,
  Separator,
  input,
  selectWithCancel,
} from '../utils/prompts.js';
import { clearScreen } from '../utils/platform.js';
import {
  runInstallFlow,
  checkAndPrintEnvironmentWithLoader,
  hasEnvironmentIssues,
} from './install/index.js';
import { runConfigOptionsFlow } from './config/index.js';
import { printGoodbye, printWelcome } from './header.js';
import { Spinner } from '../utils/spinner.js';
import { runSkillsMenu } from './skills-menu/index.js';
import { runOctocodeSkillsFlow } from './skills-menu/marketplace.js';
import { getAppState, type AppState, type SkillsState } from './state.js';
import { MCP_CLIENTS, type ClientInstallStatus } from '../utils/mcp-config.js';
import {
  login as oauthLogin,
  logout as oauthLogout,
  getAuthStatusAsync,
  getStoragePath,
  type VerificationInfo,
} from '../features/github-oauth.js';
import type { OctocodeAuthStatus } from '../types/index.js';
import { checkGitHubAuth, runGitHubAuthLogout } from '../features/gh-auth.js';
import { getGhCliToken } from '../utils/token-storage.js';
import { getCredentials, hasEnvToken } from '../utils/token-storage.js';
import open from 'open';
import { runToolTerminalFlow } from './tool-terminal.js';

async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

type MenuChoice =
  | 'octocode'
  | 'octocode-skills'
  | 'skills'
  | 'auth'
  | 'terminal'
  | 'exit';

type OctocodeMenuChoice = 'configure' | 'install' | 'back';

function getClientNames(clients: ClientInstallStatus[]): string {
  return clients.map(c => MCP_CLIENTS[c.client]?.name || c.client).join(', ');
}

function printInstalledIDEs(installedClients: ClientInstallStatus[]): void {
  if (installedClients.length === 0) {
    console.log(`  ${dim('No IDEs configured yet')}`);
    return;
  }

  console.log(`  ${dim('Installed on:')}`);
  for (const client of installedClients) {
    const clientName = MCP_CLIENTS[client.client]?.name || client.client;
    console.log(
      `    ${dim('•')} ${dim(clientName)} ${dim('->')} ${c('cyan', client.configPath)}`
    );
  }
}

function buildSkillsMenuItem(skills: SkillsState): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  if (!skills.sourceExists || !skills.hasSkills) {
    return {
      name: `${dim('- Manage System Skills')}`,
      value: 'skills',
      description: dim('Not available — no skill sources found'),
    };
  }

  if (skills.allInstalled) {
    return {
      name: `- Manage System Skills ${c('green', '✅')}`,
      value: 'skills',
      description: `${skills.totalInstalledCount} installed • Research, PR Review & more`,
    };
  }

  if (skills.totalInstalledCount > 0) {
    return {
      name: '- Manage System Skills',
      value: 'skills',
      description: `${skills.totalInstalledCount}/${skills.skills.length} installed • Get more skills!`,
    };
  }

  return {
    name: `- ${bold('Manage System Skills')} ${c('cyan', '[NEW]')}`,
    value: 'skills',
    description: `Install skills for AI-powered coding workflows`,
  };
}

function buildOctocodeSkillsMenuItem(skills: SkillsState): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  const octocodeSkillsInstalled = skills.skills.filter(
    s => s.name.startsWith('octocode-') && s.installed
  ).length;

  if (octocodeSkillsInstalled > 0) {
    return {
      name: `- Octocode Skills ${c('green', '✅')}`,
      value: 'octocode-skills',
      description: `${octocodeSkillsInstalled} installed • Research, planning & review`,
    };
  }

  return {
    name: '- Octocode Skills',
    value: 'octocode-skills',
    description: 'Install AI-powered research, planning & review skills',
  };
}

function getAuthSourceDisplay(auth: OctocodeAuthStatus): string {
  switch (auth.tokenSource) {
    case 'gh-cli':
      return 'gh CLI';
    case 'env': {
      if (auth.envTokenSource) {
        const varName = auth.envTokenSource.replace('env:', '');
        return `env (${varName})`;
      }
      return 'env var';
    }
    case 'octocode':
      return 'Octocode';
    default:
      return 'unknown';
  }
}

function buildAuthMenuItem(auth: OctocodeAuthStatus): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  if (auth.authenticated) {
    const source = getAuthSourceDisplay(auth);
    const user = auth.username ? `@${auth.username}` : '';
    const userPart = user ? `${user} ` : '';
    return {
      name: `- Manage Auth ${c('green', '✅')}`,
      value: 'auth',
      description: `${userPart}via ${source}`,
    };
  }

  return {
    name: `- ${bold('Manage Auth')} ${c('red', '[Required]')}`,
    value: 'auth',
    description: `Sign in to access GitHub`,
  };
}

function buildStatusLine(state: AppState): string {
  const parts: string[] = [];

  if (state.octocode.isInstalled) {
    const clientLabel =
      state.octocode.installedCount === 1 ? 'client' : 'clients';
    parts.push(
      `${c('green', '●')} ${state.octocode.installedCount} ${clientLabel}`
    );
  } else {
    parts.push(`${c('yellow', '○')} Not installed`);
  }

  if (state.skills.totalInstalledCount > 0) {
    parts.push(`${c('green', '●')} ${state.skills.totalInstalledCount} skills`);
  } else if (state.skills.sourceExists && state.skills.hasSkills) {
    parts.push(`${c('yellow', '○')} ${state.skills.skills.length} skills`);
  }

  return parts.join(dim('  │  '));
}

function buildOctocodeMenuItem(state: AppState): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  if (state.octocode.isInstalled) {
    const clientLabel = state.octocode.installedCount === 1 ? 'IDE' : 'IDEs';

    if (state.githubAuth.authenticated) {
      return {
        name: `- Octocode MCP ${c('green', '✅')}`,
        value: 'octocode',
        description: `Configure Octocode MCP - ${state.octocode.installedCount} ${clientLabel} configured`,
      };
    }

    return {
      name: `- Octocode MCP ${c('red', '[X]')}`,
      value: 'octocode',
      description: `Configure Octocode MCP - ${state.octocode.installedCount} ${clientLabel} configured`,
    };
  }

  return {
    name: `- ${bold('Octocode Configuration')}`,
    value: 'octocode',
    description: 'Configure Octocode MCP - 0 IDEs configured',
  };
}

function printContextualHints(state: AppState): void {
  if (!state.githubAuth.authenticated) {
    console.log();
    console.log(
      `  ${c('yellow', 'Warning:')} ${bold('Auth required!')} Run ${c('cyan', 'Manage Auth')} to access GitHub repos`
    );
  } else if (
    state.octocode.isInstalled &&
    state.skills.totalInstalledCount === 0
  ) {
    console.log();
    console.log(
      `  ${c('cyan', 'Tip:')} ${dim('Boost your AI coding:')} Install ${c('magenta', 'Skills')} for research, PR review & more!`
    );
  }
}

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
    new Separator() as unknown as {
      name: string;
      value: MenuChoice;
    }
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
    new Separator() as unknown as {
      name: string;
      value: OctocodeMenuChoice;
      description?: string;
    }
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

async function runOctocodeFlow(): Promise<void> {
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

type AuthMenuChoice = 'login' | 'gh-guidance' | 'logout' | 'gh-logout' | 'back';

async function showAuthMenu(
  status: OctocodeAuthStatus
): Promise<AuthMenuChoice> {
  const choices: Array<{
    name: string;
    value: AuthMenuChoice;
    description?: string;
  }> = [];

  const isUsingEnv = status.tokenSource === 'env';

  const ghCliToken = await getGhCliToken();
  const ghAuth = checkGitHubAuth();
  const octocodeCredentials = await getCredentials();

  const hasGhCli = !!ghCliToken;
  const hasOctocode = !!octocodeCredentials;
  const hasEnv = hasEnvToken();

  if (isUsingEnv && hasEnv) {
    const envVar =
      status.envTokenSource?.replace('env:', '') || 'environment variable';
    choices.push(
      new Separator(
        `  ${c('green', '✅')} Using ${c('cyan', envVar)} ${dim('(takes priority)')}`
      ) as unknown as {
        name: string;
        value: AuthMenuChoice;
        description?: string;
      }
    );

    choices.push(
      new Separator() as unknown as {
        name: string;
        value: AuthMenuChoice;
        description?: string;
      }
    );
  }

  if (hasGhCli) {
    const userPart = ghAuth.username ? ` (@${ghAuth.username})` : '';
    choices.push({
      name: `- Delete gh CLI token${userPart}`,
      value: 'gh-logout',
      description: 'Opens gh auth logout',
    });
  }

  if (hasOctocode) {
    const userPart = octocodeCredentials.username
      ? ` (@${octocodeCredentials.username})`
      : '';
    const storageType = 'file';
    choices.push({
      name: `- Delete Octocode token${userPart}`,
      value: 'logout',
      description: `Remove from ${storageType}`,
    });
  }

  if (!hasOctocode) {
    choices.push({
      name: `- Sign In via Octocode ${c('green', '(Recommended)')}`,
      value: 'login',
      description: 'Quick browser sign in',
    });
  }

  if (!hasGhCli) {
    choices.push({
      name: '- Sign In via gh CLI',
      value: 'gh-guidance',
      description: ghAuth.installed
        ? 'Use existing GitHub CLI'
        : 'GitHub CLI not installed',
    });
  }

  choices.push(
    new Separator() as unknown as {
      name: string;
      value: AuthMenuChoice;
      description?: string;
    }
  );

  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const choice = await selectWithCancel<AuthMenuChoice>({
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

async function runLoginFlow(): Promise<boolean> {
  console.log();
  console.log(c('blue', '━'.repeat(66)));
  console.log(`  ${bold('GitHub Authentication')}`);
  console.log(c('blue', '━'.repeat(66)));
  console.log();
  console.log(
    `  ${dim('This will open your browser to authenticate with GitHub.')}`
  );
  console.log();

  let verificationShown = false;
  let authSpinner: Spinner | null = null;
  const spinner = new Spinner('Connecting to GitHub...').start();

  const result = await oauthLogin({
    onVerification: (verification: VerificationInfo) => {
      spinner.stop();
      verificationShown = true;

      console.log();
      console.log(c('yellow', '  ┌' + '─'.repeat(50) + '┐'));
      console.log(
        c('yellow', '  │ ') +
          `${c('yellow', '!')} Your one-time code: ${bold(c('cyan', verification.user_code))}` +
          ' '.repeat(50 - 26 - verification.user_code.length) +
          c('yellow', '│')
      );
      console.log(c('yellow', '  └' + '─'.repeat(50) + '┘'));
      console.log();
      console.log(`  ${bold('1.')} Copy the code above`);
      console.log(
        `  ${bold('2.')} ${bold('Press Enter')} to open ${c('cyan', verification.verification_uri)}`
      );
      console.log(`  ${bold('3.')} Paste the code in your browser`);
      console.log();

      authSpinner = new Spinner(
        `Waiting for browser authentication... ${dim('(typically 10-30 seconds)')}`
      ).start();
    },
  });

  if (authSpinner) {
    (authSpinner as Spinner).stop();
  }
  if (!verificationShown) {
    spinner.stop();
  }

  console.log();
  if (result.success) {
    console.log(c('green', '  ┌' + '─'.repeat(50) + '┐'));
    console.log(
      c('green', '  │ ') +
        `${c('green', '✅')} ${bold('Authentication successful!')}` +
        ' '.repeat(22) +
        c('green', '│')
    );
    console.log(c('green', '  └' + '─'.repeat(50) + '┘'));
    console.log();
    console.log(
      `  ${c('green', '✅')} Logged in as ${c('cyan', '@' + (result.username || 'unknown'))}`
    );
    console.log(`  ${dim('Credentials stored in:')} ${getStoragePath()}`);
    console.log();
    console.log(`  ${c('cyan', 'Tip:')} ${bold("What's next?")}`);
    console.log(
      `     ${dim('•')} Install ${c('magenta', 'Skills')} for AI-powered research & PR reviews`
    );
    console.log(
      `     ${dim('•')} Use ${c('cyan', '/research')} prompt to explore any GitHub repo`
    );
    console.log(
      `     ${dim('•')} Add ${c('cyan', 'AGENTS.md')} to your project for better AI context`
    );
  } else {
    console.log(c('red', '  ┌' + '─'.repeat(50) + '┐'));
    console.log(
      c('red', '  │ ') +
        `${c('red', 'X')} ${bold('Authentication failed')}` +
        ' '.repeat(27) +
        c('red', '│')
    );
    console.log(c('red', '  └' + '─'.repeat(50) + '┘'));
    console.log();
    console.log(`  ${c('red', 'Error:')} ${result.error || 'Unknown error'}`);
    console.log();
    console.log(`  ${bold('Troubleshooting:')}`);
    console.log(`     ${dim('•')} Make sure you copied the code correctly`);
    console.log(`     ${dim('•')} Check your browser didn't block the popup`);
    console.log(
      `     ${dim('•')} Try running ${c('cyan', 'octocode login')} again`
    );
  }
  console.log();

  await pressEnterToContinue();
  return result.success;
}

async function runLogoutFlow(): Promise<boolean> {
  const status = await getAuthStatusAsync();

  console.log();
  console.log(`  ${bold('Sign Out')}`);
  console.log(
    `  ${dim('Signed in as:')} ${c('cyan', '@' + (status.username || 'unknown'))}`
  );
  console.log();

  const result = await oauthLogout();

  if (result.success) {
    console.log(`  ${c('green', '✅')} Signed out successfully`);

    const ghAuth = checkGitHubAuth();
    if (ghAuth.authenticated) {
      console.log(
        `  ${dim('Tip:')} You can still use gh CLI (@${ghAuth.username || 'unknown'})`
      );
    }
  } else {
    console.log(
      `  ${c('red', 'X')} Sign out failed: ${result.error || 'Unknown error'}`
    );
  }
  console.log();

  await pressEnterToContinue();
  return result.success;
}

type GhGuidanceChoice = 'open-site' | 'back';

async function showGhCliGuidance(): Promise<void> {
  const GH_CLI_URL = 'https://cli.github.com/';

  console.log();
  console.log(`  ${bold('Setup Instructions:')}`);
  console.log();
  console.log(`  1. Install GitHub CLI from:`);
  console.log(`     ${c('cyan', GH_CLI_URL)}`);
  console.log();
  console.log(`  2. Run the following command to authenticate:`);
  console.log(`     ${c('cyan', 'gh auth login')}`);
  console.log();
  console.log(`  ${dim('Once authenticated, octocode will automatically')}`);
  console.log(`  ${dim('use your gh CLI token.')}`);
  console.log();

  const choice = await selectWithCancel<GhGuidanceChoice>({
    message: '',
    choices: [
      {
        name: '- Open GitHub CLI website',
        value: 'open-site',
      },
      new Separator() as unknown as {
        name: string;
        value: GhGuidanceChoice;
      },
      {
        name: `${c('dim', '- Back')}`,
        value: 'back',
      },
    ],
    pageSize: 10,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
        message: (text: string) => text,
      },
    },
  });

  if (choice === 'back') {
    return;
  }

  if (choice === 'open-site') {
    try {
      await open(GH_CLI_URL);
      console.log();
      console.log(
        `  ${c('green', '✅')} Opened ${c('cyan', GH_CLI_URL)} in browser`
      );
    } catch {
      console.log();
      console.log(`  ${c('yellow', '!')} Could not open browser automatically`);
      console.log(`  ${dim('Please visit:')} ${c('cyan', GH_CLI_URL)}`);
    }
    console.log();
    await pressEnterToContinue();
  }
}

function getDetailedAuthSource(status: OctocodeAuthStatus): string {
  switch (status.tokenSource) {
    case 'gh-cli':
      return 'gh CLI';
    case 'env': {
      if (status.envTokenSource) {
        const varName = status.envTokenSource.replace('env:', '');
        return `${varName} env var`;
      }
      return 'environment variable';
    }
    case 'octocode':
      return 'file';
    default:
      return 'unknown';
  }
}

function displayAuthStatus(status: OctocodeAuthStatus): void {
  console.log(`  ${bold('GitHub Authentication')}`);
  console.log();

  if (status.authenticated) {
    const source = getDetailedAuthSource(status);

    if (status.tokenSource === 'env') {
      const envVarName = status.envTokenSource
        ? status.envTokenSource.replace('env:', '')
        : 'environment variable';
      console.log(
        `  ${c('green', '✅')} Using ${c('cyan', envVarName)} ${dim('(token configured)')}`
      );
    } else {
      console.log(
        `  ${c('green', '✅')} Signed in as ${c('cyan', '@' + (status.username || 'unknown'))} ${dim(`via ${source}`)}`
      );
    }

    if (status.tokenExpired) {
      console.log(
        `  ${c('yellow', 'Warning:')} Session expired - please sign in again`
      );
    }

    console.log();
    console.log(
      `  ${c('green', '✅')} ${dim('Ready to access GitHub repositories!')}`
    );
  } else {
    console.log(c('yellow', '  ┌' + '─'.repeat(56) + '┐'));
    console.log(
      c('yellow', '  │ ') +
        `${c('yellow', 'Warning:')} ${bold('Authentication Required')}` +
        ' '.repeat(31) +
        c('yellow', '│')
    );
    console.log(c('yellow', '  └' + '─'.repeat(56) + '┘'));
    console.log();
    console.log(`  ${dim('Without auth, Octocode cannot:')}`);
    console.log(`     ${c('red', 'X')} Access private repositories`);
    console.log(`     ${c('red', 'X')} Search code in your organization`);
    console.log(
      `     ${c('red', 'X')} Provide full GitHub research capabilities`
    );
    console.log();
    console.log(
      `  ${c('cyan', '->')} Select ${c('green', '"Sign In via Octocode"')} below to authenticate`
    );
  }
  console.log();
}

async function runAuthFlow(): Promise<void> {
  await loadInquirer();
  console.log();

  let inAuthMenu = true;
  while (inAuthMenu) {
    const status = await getAuthStatusAsync();

    displayAuthStatus(status);

    const choice = await showAuthMenu(status);

    switch (choice) {
      case 'login': {
        const success = await runLoginFlow();
        console.log();
        if (success) {
          inAuthMenu = false;
        }
        break;
      }

      case 'gh-guidance':
        await showGhCliGuidance();
        break;

      case 'logout':
        await runLogoutFlow();
        console.log();
        break;

      case 'gh-logout': {
        const confirmGh = await selectWithCancel<'yes' | 'no'>({
          message: 'Sign out of gh CLI?',
          choices: [
            { name: 'Yes, sign out', value: 'yes' },
            { name: 'No, cancel', value: 'no' },
          ],
          theme: {
            prefix: '  ',
            style: {
              highlight: (text: string) => c('red', text),
            },
          },
        });

        if (confirmGh !== 'yes') {
          break;
        }

        console.log();
        console.log(`  ${dim('Opening gh auth logout...')}`);
        console.log();
        const ghResult = runGitHubAuthLogout();
        if (ghResult.success) {
          console.log();
          console.log(`  ${c('green', '✅')} Signed out of gh CLI`);
        } else {
          console.log();
          console.log(`  ${c('yellow', '!')} Sign out was cancelled`);
        }
        console.log();
        await pressEnterToContinue();
        break;
      }

      case 'back':
      default:
        inAuthMenu = false;
        break;
    }
  }
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
