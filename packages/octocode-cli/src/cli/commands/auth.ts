import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import {
  login as oauthLogin,
  logout as oauthLogout,
  getAuthStatus,
  getStoragePath,
  type VerificationInfo,
} from '../../features/github-oauth.js';
import { loadInquirer, select } from '../../utils/prompts.js';
import { Spinner } from '../../utils/spinner.js';
import { printAuthStatus, printLoginHint } from './shared.js';
import { tokenCommand } from './token.js';

export const loginCommand: CLICommand = {
  name: 'login',
  aliases: ['l'],
  description: 'Authenticate with GitHub',
  usage: 'octocode-cli login [--hostname <host>] [--git-protocol <ssh|https>]',
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
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const status = getAuthStatus(hostname);

    if (status.authenticated) {
      console.log();
      console.log(
        `  ${c('green', '✓')} Already authenticated as ${c('cyan', status.username || 'unknown')}`
      );
      console.log();
      console.log(`  ${dim('To switch accounts, logout first:')}`);
      console.log(
        `    ${c('cyan', '→')} ${c('yellow', 'octocode-cli logout')}`
      );
      console.log();
      return;
    }

    console.log();
    console.log(`  ${bold('🔐 GitHub Authentication')}`);
    console.log();

    const gitProtocolOpt = args.options['git-protocol'] ?? args.options['p'];
    const gitProtocol =
      typeof gitProtocolOpt === 'string' ? gitProtocolOpt : 'https';

    if (gitProtocol !== 'ssh' && gitProtocol !== 'https') {
      console.log();
      console.log(`  ${c('red', '✗')} Invalid git protocol: ${gitProtocol}`);
      console.log(`  ${dim('Supported:')} ssh, https`);
      console.log();
      process.exitCode = 1;
      return;
    }

    let verificationShown = false;

    const spinner = new Spinner('Waiting for GitHub authentication...').start();

    const result = await oauthLogin({
      hostname,
      gitProtocol,
      onVerification: (verification: VerificationInfo) => {
        spinner.stop();
        verificationShown = true;

        console.log(
          `  ${c('yellow', '!')} First copy your one-time code: ${bold(verification.user_code)}`
        );
        console.log();
        console.log(
          `  ${bold('Press Enter')} to open ${c('cyan', verification.verification_uri)} in your browser...`
        );
        console.log();
        console.log(`  ${dim('Waiting for authentication...')}`);
      },
    });

    if (!verificationShown) {
      spinner.stop();
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
  usage: 'octocode-cli logout [--hostname <host>]',
  options: [
    {
      name: 'hostname',
      short: 'H',
      description: 'GitHub Enterprise hostname',
      hasValue: true,
    },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const status = getAuthStatus(hostname);

    if (!status.authenticated) {
      console.log();
      console.log(
        `  ${c('yellow', '⚠')} Not currently authenticated to ${hostname}`
      );
      console.log();
      printLoginHint();
      console.log();
      return;
    }

    console.log();
    console.log(`  ${bold('🔐 GitHub Logout')}`);
    console.log(
      `  ${dim('Currently authenticated as:')} ${c('cyan', status.username || 'unknown')}`
    );
    console.log();

    const result = await oauthLogout(hostname);

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
  usage: 'octocode-cli auth [login|logout|status|token]',
  handler: async (args: ParsedArgs) => {
    const subcommand = args.args[0];
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';

    if (subcommand === 'login') {
      return loginCommand.handler(args);
    }
    if (subcommand === 'logout') {
      return logoutCommand.handler(args);
    }
    if (subcommand === 'status') {
      printAuthStatus(hostname);
      return;
    }
    if (subcommand === 'token') {
      return tokenCommand.handler(args);
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
