import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import {
  login as oauthLogin,
  logout as oauthLogout,
  getAuthStatus,
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
import { tokenCommand } from './token.js';

export const loginCommand: CLICommand = {
  name: 'login',
  aliases: ['l'],
  description: 'Authenticate with GitHub',
  usage:
    'octocode login [--hostname <host>] [--git-protocol <ssh|https>] [--force] [--json]',
  options: [
    {
      name: 'hostname',
      short: 'H',
      description: 'GitHub Enterprise hostname (default: github.com)',
      hasValue: true,
    },
    {
      name: 'git-protocol',
      short: 'p',
      description: 'Git protocol to use (ssh or https)',
      hasValue: true,
    },
    {
      name: 'force',
      short: 'f',
      description:
        'Re-authenticate even if already logged in (logout then login)',
    },
    {
      name: 'json',
      short: 'j',
      description: 'Output result as JSON: { success, username, error }',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json'] || args.options['j']);
    const forceLogin = Boolean(args.options['force'] || args.options['f']);
    const status = getAuthStatus(hostname);

    if (status.authenticated && !forceLogin) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: true,
            username: status.username || null,
            error: null,
            alreadyAuthenticated: true,
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
      console.log(
        `    ${c('cyan', '→')} ${c('yellow', 'octocode login --force')}`
      );
      console.log();
      return;
    }

    if (forceLogin && status.authenticated) {
      if (!jsonOutput) {
        console.log();
        console.log(
          `  ${dim('Logging out')} ${c('cyan', status.username || hostname)} ${dim('before re-authenticating...')}`
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
              'Login requires browser interaction. Run "octocode login" in an interactive terminal.',
            requiresInteraction: true,
          })
        );
      } else {
        console.log();
        console.log(`  ${c('red', '✗')} Login requires browser interaction.`);
        console.log(
          `  ${dim('Run')} ${c('yellow', 'octocode login')} ${dim('in an interactive terminal.')}`
        );
        console.log();
      }
      process.exitCode = 1;
      return;
    }

    const gitProtocolOpt = args.options['git-protocol'] ?? args.options['p'];
    const gitProtocol =
      typeof gitProtocolOpt === 'string' ? gitProtocolOpt : 'https';

    if (gitProtocol !== 'ssh' && gitProtocol !== 'https') {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            username: null,
            error: `Invalid git protocol: ${gitProtocol}. Supported: ssh, https`,
          })
        );
        process.exitCode = 1;
        return;
      }
      console.log();
      console.log(`  ${c('red', '✗')} Invalid git protocol: ${gitProtocol}`);
      console.log(`  ${dim('Supported:')} ssh, https`);
      console.log();
      process.exitCode = 1;
      return;
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
      if (!result.success) process.exitCode = 1;
      return;
    }

    console.log();
    if (result.success) {
      console.log(`  ${c('green', '✓')} Authentication complete!`);
      console.log(
        `  ${c('green', '✓')} Logged in as ${c('cyan', result.username || 'unknown')}`
      );
      console.log();
      console.log(`  ${dim('Credentials stored in:')} ${getStoragePath()}`);
    } else {
      console.log(
        `  ${c('red', '✗')} Authentication failed: ${result.error || 'Unknown error'}`
      );
      process.exitCode = 1;
    }
    console.log();
  },
};

export const logoutCommand: CLICommand = {
  name: 'logout',
  description: 'Sign out from GitHub',
  usage: 'octocode logout [--hostname <host>] [--yes] [--json]',
  options: [
    {
      name: 'hostname',
      short: 'H',
      description: 'GitHub Enterprise hostname',
      hasValue: true,
    },
    {
      name: 'yes',
      short: 'y',
      description: 'Skip confirmation prompt',
    },
    {
      name: 'json',
      short: 'j',
      description: 'Output result as JSON: { success, hostname, error }',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json'] || args.options['j']);
    const skipConfirm = Boolean(args.options['yes'] || args.options['y']);
    const status = getAuthStatus(hostname);

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
      if (!result.success) process.exitCode = 1;
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
        `  ${c('green', '✓')} Successfully logged out from ${hostname}`
      );
    } else {
      console.log(
        `  ${c('red', '✗')} Logout failed: ${result.error || 'Unknown error'}`
      );
      process.exitCode = 1;
    }
    console.log();
  },
};

export const authCommand: CLICommand = {
  name: 'auth',
  aliases: ['a', 'gh'],
  description: 'Manage GitHub authentication',
  usage: 'octocode auth [login|logout|status|token|refresh] [--json]',
  options: [
    {
      name: 'hostname',
      short: 'H',
      description: 'GitHub Enterprise hostname (default: github.com)',
      hasValue: true,
    },
    {
      name: 'json',
      short: 'j',
      description: 'Output as JSON (supported by all subcommands)',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = args.args[0];
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json'] || args.options['j']);

    if (subcommand === 'login') {
      return loginCommand.handler(args);
    }
    if (subcommand === 'logout') {
      return logoutCommand.handler(args);
    }
    if (subcommand === 'status') {
      if (jsonOutput) {
        const data = formatAuthStatusAsJson(hostname);
        console.log(JSON.stringify(data));
        if (!data['authenticated']) process.exitCode = 1;
        return;
      }
      printAuthStatus(hostname);
      return;
    }
    if (subcommand === 'token') {
      return tokenCommand.handler(args);
    }
    if (subcommand === 'refresh') {
      const currentStatus = getAuthStatus(hostname);
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
        process.exitCode = 1;
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
        process.exitCode = 1;
        return;
      }

      if (tokenSource === 'none' || !currentStatus.authenticated) {
        const msg = 'Not authenticated. Run `octocode login` first.';
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
        process.exitCode = 1;
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
        if (!result.success) process.exitCode = 1;
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
          `  ${dim('Tip:')} run ${c('yellow', 'octocode login')} to re-authenticate`
        );
        process.exitCode = 1;
      }
      console.log();
      return;
    }

    if (!process.stdout.isTTY) {
      if (jsonOutput) {
        const data = formatAuthStatusAsJson(hostname);
        console.log(JSON.stringify(data));
        if (!data['authenticated']) process.exitCode = 1;
      } else {
        printAuthStatus(hostname);
      }
      return;
    }

    const status = getAuthStatus(hostname);
    printAuthStatus(hostname);

    await loadInquirer();

    const choices = status.authenticated
      ? [
          { name: '🔓 Logout from GitHub', value: 'logout' },
          { name: '🔄 Switch account (logout & login)', value: 'switch' },
          { name: '← Back', value: 'back' },
        ]
      : [
          { name: '🔐 Login to GitHub', value: 'login' },
          { name: '← Back', value: 'back' },
        ];

    const action = await select({
      message: 'What would you like to do?',
      choices,
    });

    if (action === 'login') {
      await loginCommand.handler({
        command: 'login',
        args: [],
        options: { hostname },
      });
    } else if (action === 'logout') {
      await oauthLogout(hostname);
      console.log();
      console.log(`  ${c('green', '✓')} Successfully logged out`);
      console.log();
    } else if (action === 'switch') {
      console.log();
      console.log(`  ${dim('Logging out...')}`);
      await oauthLogout(hostname);
      console.log(`  ${c('green', '✓')} Logged out`);
      console.log();
      console.log(`  ${dim('Starting new login...')}`);
      await loginCommand.handler({
        command: 'login',
        args: [],
        options: { hostname },
      });
    }
  },
};
