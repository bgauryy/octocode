import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import {
  login as oauthLogin,
  logout as oauthLogout,
  getAuthStatusAsync,
  getStoragePath,
  refreshAuthToken,
  type VerificationInfo,
} from '../../features/github-oauth.js';
import { loadInquirer, select } from '../../utils/prompts.js';
import { Spinner } from '../../utils/spinner.js';
import {
  formatAuthStatusAsJson,
  printAuthStatus,
  printLoginHint,
} from './shared.js';
import {
  GH_CLI_URL,
  checkGitHubAuth,
  runGitHubAuthLogin,
  runGitHubAuthLogout,
} from '../../features/gh-auth.js';
import {
  getCredentials,
  hasEnvToken,
  getEnvTokenSource,
} from '../../utils/token-storage.js';

type AuthMenuAction =
  'login' | 'logout' | 'switch' | 'gh-login' | 'gh-logout' | 'back';

function isOctocodeAuthStatus(
  status: Awaited<ReturnType<typeof getAuthStatusAsync>>
): boolean {
  return status.tokenSource === 'octocode';
}

function normalizeGitProtocol(
  value: ParsedArgs['options'][string]
): 'ssh' | 'https' | null {
  const gitProtocol = typeof value === 'string' ? value : 'https';
  return gitProtocol === 'ssh' || gitProtocol === 'https' ? gitProtocol : null;
}

function printInvalidGitProtocol(
  gitProtocol: string,
  jsonOutput: boolean
): void {
  if (jsonOutput) {
    console.log(
      JSON.stringify({
        success: false,
        username: null,
        error: `Invalid git protocol: ${gitProtocol}. Supported: ssh, https`,
      })
    );
    process.exitCode = EXIT.USAGE;
    return;
  }
  console.log();
  console.log(`  ${c('red', '✗')} Invalid git protocol: ${gitProtocol}`);
  console.log(`  ${dim('Supported:')} ssh, https`);
  console.log();
  process.exitCode = EXIT.USAGE;
}

async function runOctocodeLogin(args: ParsedArgs): Promise<void> {
  const hostnameOpt = args.options['hostname'];
  const hostname =
    (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) || 'github.com';
  const jsonOutput = Boolean(args.options['json']);
  const forceLogin = Boolean(args.options['force']);
  const status = await getAuthStatusAsync(hostname);
  const alreadyOctocode = isOctocodeAuthStatus(status);

  if (alreadyOctocode && !forceLogin) {
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: true,
          username: status.username || null,
          error: null,
          alreadyAuthenticated: true,
          tokenSource: 'octocode',
        })
      );
      return;
    }
    console.log();
    console.log(
      `  ${c('green', '✓')} Already authenticated as ${c('cyan', status.username || 'unknown')}`
    );
    console.log();
    console.log(`  ${dim('To switch accounts, use --force:')}`);
    console.log(`    ${c('cyan', '→')} ${c('yellow', 'login --force')}`);
    console.log();
    return;
  }

  if (forceLogin && alreadyOctocode) {
    if (!jsonOutput) {
      console.log();
      console.log(
        `  ${dim('Signing out')} ${c('cyan', status.username || hostname)} ${dim('before re-authenticating...')}`
      );
    }
    await oauthLogout(hostname);
  }

  if (!process.stdout.isTTY) {
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: false,
          error:
            'Login requires browser interaction. Run "login" in an interactive terminal.',
          requiresInteraction: true,
        })
      );
    } else {
      console.log();
      console.log(`  ${c('red', '✗')} Login requires browser interaction.`);
      console.log(
        `  ${dim('Run')} ${c('yellow', 'login')} ${dim('in an interactive terminal.')}`
      );
      console.log();
    }
    process.exitCode = EXIT.USAGE;
    return;
  }

  const gitProtocolOpt = args.options['git-protocol'];
  const gitProtocol = normalizeGitProtocol(gitProtocolOpt);
  if (!gitProtocol) {
    printInvalidGitProtocol(String(gitProtocolOpt), jsonOutput);
    return;
  }

  if (hasEnvToken()) {
    const envSource = getEnvTokenSource();
    const envVar = envSource?.replace('env:', '') || 'environment variable';
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          step: 'warning',
          envVar,
          message: `${envVar} is set and takes priority — stored OAuth token won't be used until you unset it`,
        })
      );
    } else {
      console.log();
      console.log(
        `  ${c('yellow', '⚠')} ${bold(envVar)} is set and takes priority over stored credentials.`
      );
      console.log(
        `  ${dim("Your new OAuth token will be stored but won't be used until you unset that variable.")}`
      );
    }
  }

  if (!jsonOutput) {
    console.log();
    console.log(`  ${bold('🔐 GitHub Authentication')}`);
    console.log();
  }

  let verificationShown = false;
  const spinner = jsonOutput
    ? null
    : new Spinner('Waiting for GitHub authentication...').start();

  const result = await oauthLogin({
    hostname,
    gitProtocol,
    onVerification: (verification: VerificationInfo) => {
      spinner?.stop();
      verificationShown = true;
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            step: 'verification',
            userCode: verification.user_code,
            verificationUri: verification.verification_uri,
            expiresIn: verification.expires_in,
          })
        );
      } else {
        console.log(
          `  ${c('yellow', '!')} First copy your one-time code: ${bold(verification.user_code)}`
        );
        console.log();
        console.log(
          `  ${bold('Opening')} ${c('cyan', verification.verification_uri)} ${bold('in your browser...')}`
        );
        console.log();
        console.log(`  ${dim('Waiting for authentication...')}`);
      }
    },
  });

  if (!verificationShown) {
    spinner?.stop();
  }

  if (jsonOutput) {
    console.log(
      JSON.stringify({
        step: 'result',
        success: result.success,
        username: result.username || null,
        error: result.error || null,
      })
    );
    if (!result.success) process.exitCode = EXIT.AUTH;
    return;
  }

  console.log();
  if (result.success) {
    console.log(`  ${c('green', '✓')} Authentication complete!`);
    console.log(
      `  ${c('green', '✓')} Signed in as ${c('cyan', result.username || 'unknown')}`
    );
    console.log();
    console.log(`  ${dim('Credentials stored in:')} ${getStoragePath()}`);
  } else {
    console.log(
      `  ${c('red', '✗')} Authentication failed: ${result.error || 'Unknown error'}`
    );
    process.exitCode = EXIT.AUTH;
  }
  console.log();
}

async function runGhLogin(
  hostname: string,
  gitProtocol: 'ssh' | 'https'
): Promise<void> {
  const ghAuth = checkGitHubAuth();
  if (!ghAuth.installed) {
    console.log();
    console.log(`  ${c('yellow', '!')} GitHub CLI is not installed.`);
    console.log(`  ${dim('Install:')} ${c('cyan', GH_CLI_URL)}`);
    console.log();
    process.exitCode = EXIT.USAGE;
    return;
  }

  console.log();
  console.log(`  ${dim('Opening gh auth login...')}`);
  console.log();
  const result = runGitHubAuthLogin({
    web: true,
    hostname,
    gitProtocol,
  });
  if (result.success) {
    console.log();
    console.log(`  ${c('green', '✓')} gh CLI authentication complete`);
  } else {
    console.log();
    console.log(`  ${c('red', '✗')} gh CLI authentication did not complete`);
    process.exitCode = EXIT.AUTH;
  }
  console.log();
}

async function runGhLogout(hostname: string): Promise<void> {
  console.log();
  console.log(`  ${dim('Opening gh auth logout...')}`);
  console.log();
  const result = runGitHubAuthLogout(hostname);
  if (result.success) {
    console.log();
    console.log(`  ${c('green', '✓')} Signed out of gh CLI`);
  } else {
    console.log();
    console.log(`  ${c('yellow', '!')} gh CLI sign-out did not complete`);
    process.exitCode = EXIT.AUTH;
  }
  console.log();
}

async function runAuthMenu(args: ParsedArgs): Promise<void> {
  const hostnameOpt = args.options['hostname'];
  const hostname =
    (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) || 'github.com';
  const gitProtocolOpt = args.options['git-protocol'];
  const gitProtocol = normalizeGitProtocol(gitProtocolOpt);
  if (!gitProtocol) {
    printInvalidGitProtocol(String(gitProtocolOpt), false);
    return;
  }

  await printAuthStatus(hostname);
  await loadInquirer();

  const status = await getAuthStatusAsync(hostname);
  const ghAuth = checkGitHubAuth();
  const octocodeCredentials = await getCredentials(hostname);
  const hasOctocode =
    Boolean(octocodeCredentials) || isOctocodeAuthStatus(status);
  const hasGhCli = ghAuth.authenticated;
  const choices: Array<
    | { name: string; value: AuthMenuAction; description?: string }
    | { type: 'separator'; separator?: string }
  > = [];

  if (status.tokenSource === 'env' || hasEnvToken()) {
    const envVar =
      status.envTokenSource?.replace('env:', '') || 'environment variable';
    choices.push({
      type: 'separator',
      separator: `Using ${envVar} (takes priority)`,
    });
  }

  if (hasOctocode) {
    const userPart =
      octocodeCredentials?.username || status.tokenSource === 'octocode'
        ? ` (${octocodeCredentials?.username || status.username || 'unknown'})`
        : '';
    choices.push({
      name: `Switch Octocode account${userPart}`,
      value: 'switch',
      description: 'Sign out from Octocode storage, then sign in again',
    });
    choices.push({
      name: `Delete Octocode token${userPart}`,
      value: 'logout',
      description: `Remove credentials from ${getStoragePath()}`,
    });
  } else {
    choices.push({
      name: `Sign in via Octocode ${c('green', '(Recommended)')}`,
      value: 'login',
      description: 'Browser OAuth, stored by Octocode',
    });
  }

  if (hasGhCli) {
    const userPart = ghAuth.username ? ` (@${ghAuth.username})` : '';
    choices.push({
      name: `Delete gh CLI token${userPart}`,
      value: 'gh-logout',
      description: 'Run gh auth logout',
    });
  } else {
    choices.push({
      name: 'Sign in via gh CLI',
      value: 'gh-login',
      description: ghAuth.installed
        ? 'Run gh auth login'
        : 'GitHub CLI not installed',
    });
  }

  if (!hasEnvToken()) {
    choices.push({
      type: 'separator',
      separator:
        'Or set OCTOCODE_TOKEN / GITHUB_TOKEN / GITHUB_PERSONAL_ACCESS_TOKEN env var',
    });
  }
  choices.push({ name: 'Back', value: 'back' });

  const action = await select<AuthMenuAction>({
    message: 'Choose an authentication method',
    choices,
  });

  if (action === 'login') {
    await runOctocodeLogin(args);
  } else if (action === 'logout') {
    await oauthLogout(hostname);
    console.log();
    console.log(`  ${c('green', '✓')} Successfully signed out`);
    console.log();
  } else if (action === 'switch') {
    console.log();
    console.log(`  ${dim('Signing out...')}`);
    await oauthLogout(hostname);
    console.log(`  ${c('green', '✓')} Signed out`);
    console.log();
    console.log(`  ${dim('Starting new login...')}`);
    await runOctocodeLogin({
      command: args.command,
      args: [],
      options: { ...args.options, force: true, hostname },
    });
  } else if (action === 'gh-login') {
    await runGhLogin(hostname, gitProtocol);
  } else if (action === 'gh-logout') {
    await runGhLogout(hostname);
  }
}

export const loginCommand: CLICommand = {
  name: 'login',
  options: [
    { name: 'hostname', hasValue: true },
    { name: 'git-protocol', hasValue: true },
    { name: 'force' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const jsonOutput = Boolean(args.options['json']);
    const forceLogin = Boolean(args.options['force']);
    if (!jsonOutput && !forceLogin && process.stdout.isTTY) {
      return runAuthMenu(args);
    }

    return runOctocodeLogin(args);
  },
};

export const logoutCommand: CLICommand = {
  name: 'logout',
  options: [
    { name: 'hostname', hasValue: true },
    { name: 'yes' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json']);
    const skipConfirm = Boolean(args.options['yes']);
    const status = await getAuthStatusAsync(hostname);

    if (!status.authenticated) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: true,
            hostname,
            error: null,
            alreadyLoggedOut: true,
          })
        );
        return;
      }
      console.log();
      console.log(
        `  ${c('yellow', '⚠')} Not currently authenticated to ${hostname}`
      );
      console.log();
      printLoginHint();
      console.log();
      return;
    }

    if (!skipConfirm && !jsonOutput && process.stdout.isTTY) {
      const { confirm } = await import('../../utils/prompts.js');
      const confirmed = await confirm({
        message: `Log out from ${c('cyan', status.username || hostname)}?`,
        default: false,
      });
      if (!confirmed) {
        console.log();
        console.log(`  ${dim('Logout cancelled.')}`);
        console.log();
        return;
      }
    }

    const result = await oauthLogout(hostname);

    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: result.success,
          hostname,
          error: result.error || null,
        })
      );
      if (!result.success) process.exitCode = EXIT.GENERAL;
      return;
    }

    console.log();
    console.log(`  ${bold('🔐 GitHub Logout')}`);
    console.log(
      `  ${dim('Currently authenticated as:')} ${c('cyan', status.username || 'unknown')}`
    );
    console.log();
    if (result.success) {
      console.log(
        `  ${c('green', '✓')} Successfully signed out from ${hostname}`
      );
    } else {
      console.log(
        `  ${c('red', '✗')} Logout failed: ${result.error || 'Unknown error'}`
      );
      process.exitCode = EXIT.GENERAL;
    }
    console.log();
  },
};

export const authCommand: CLICommand = {
  name: 'auth',
  options: [
    { name: 'hostname', hasValue: true },
    { name: 'json' },
    { name: 'status' },
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = args.args[0];
    const hostnameOpt = args.options['hostname'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json']);

    if (subcommand === 'login') {
      if (!jsonOutput && !args.options['force'] && process.stdout.isTTY) {
        return runAuthMenu(args);
      }
      return loginCommand.handler(args);
    }
    if (subcommand === 'logout') {
      return logoutCommand.handler(args);
    }
    if (subcommand === 'status' || args.options['status']) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: true,
            ...(await formatAuthStatusAsJson(hostname)),
          })
        );
        return;
      }

      await printAuthStatus(hostname);
      return;
    }

    if (subcommand === 'token') {
      const message =
        'auth token was removed. Use `auth status --json` to check token presence.';
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            error: message,
          })
        );
      } else {
        console.log();
        console.log(`  ${c('red', '✗')} ${message}`);
        console.log();
      }
      process.exitCode = EXIT.USAGE;
      return;
    }
    if (subcommand === 'refresh') {
      const currentStatus = await getAuthStatusAsync(hostname);
      const tokenSource = currentStatus.tokenSource;

      if (tokenSource === 'env') {
        const envVar = (currentStatus as { envTokenSource?: string })
          .envTokenSource;
        const msg = `Token is from environment variable${envVar ? ` (${envVar})` : ''} — update it directly to refresh.`;
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              hostname,
              tokenSource,
              refreshable: false,
              error: msg,
            })
          );
        } else {
          console.log();
          console.log(`  ${c('yellow', '⚠')} ${msg}`);
          console.log();
        }
        process.exitCode = EXIT.USAGE;
        return;
      }

      if (tokenSource === 'gh-cli') {
        const msg =
          'Token is managed by the gh CLI — run `gh auth refresh` instead.';
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              hostname,
              tokenSource,
              refreshable: false,
              error: msg,
              hint: 'gh auth refresh',
            })
          );
        } else {
          console.log();
          console.log(`  ${c('yellow', '⚠')} ${msg}`);
          console.log(
            `  ${dim('Run:')} ${c('cyan', 'gh auth refresh')}${hostname !== 'github.com' ? ` ${dim(`--hostname ${hostname}`)}` : ''}`
          );
          console.log();
        }
        process.exitCode = EXIT.USAGE;
        return;
      }

      if (tokenSource === 'none' || !currentStatus.authenticated) {
        const msg = 'Not authenticated. Run `login` first.';
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              hostname,
              tokenSource,
              refreshable: false,
              error: msg,
            })
          );
        } else {
          console.log();
          console.log(`  ${c('red', '✗')} ${msg}`);
          console.log();
        }
        process.exitCode = EXIT.AUTH;
        return;
      }

      const spinner = jsonOutput
        ? null
        : new Spinner('Refreshing Octocode token...').start();
      const result = await refreshAuthToken(hostname);
      spinner?.stop();
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: result.success,
            hostname,
            tokenSource,
            refreshable: true,
            username: result.username ?? null,
            error: result.error ?? null,
          })
        );
        if (!result.success) process.exitCode = EXIT.AUTH;
        return;
      }
      console.log();
      if (result.success) {
        console.log(
          `  ${c('green', '✓')} Token refreshed for ${c('cyan', result.username ?? hostname)}`
        );
      } else {
        console.log(
          `  ${c('red', '✗')} Token refresh failed: ${result.error ?? 'unknown error'}`
        );
        console.log(
          `  ${dim('Tip:')} run ${c('yellow', 'login')} to re-authenticate`
        );
        process.exitCode = EXIT.AUTH;
      }
      console.log();
      return;
    }

    if (!process.stdout.isTTY) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            error:
              'Provide an auth action: login, logout, refresh, or status. Use `auth status --json` for read-only auth state.',
          })
        );
      } else {
        console.log();
        console.log(
          `  ${c('red', '✗')} Provide an auth action: login, logout, refresh, or status.`
        );
        console.log(
          `  ${dim('Use')} ${c('cyan', 'auth status --json')} ${dim('for read-only auth state.')}`
        );
        console.log();
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    await runAuthMenu(args);
  },
};
