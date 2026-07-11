import { c, bold, dim } from '../../utils/colors.js';
import { selectWithCancel } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { Spinner } from '../../utils/spinner.js';
import {
  login as oauthLogin,
  logout as oauthLogout,
  getAuthStatusAsync,
  getStoragePath,
  type VerificationInfo,
} from '../../features/github-oauth.js';
import type { OctocodeAuthStatus } from '../../types/index.js';
import { checkGitHubAuth } from '../../features/gh-auth.js';
import open from 'open';
import { pressEnterToContinue } from './shared.js';
import type { GhGuidanceChoice } from './types.js';

export async function runLoginFlow(): Promise<boolean> {
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
      `  ${c('green', '✅')} Signed in as ${c('cyan', '@' + (result.username || 'unknown'))}`
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

export async function runLogoutFlow(): Promise<boolean> {
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

export async function showGhCliGuidance(): Promise<void> {
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
      separatorChoice<{
        name: string;
        value: GhGuidanceChoice;
      }>(),
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

export function displayAuthStatus(status: OctocodeAuthStatus): void {
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
