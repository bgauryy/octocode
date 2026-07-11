import type { CLICommand, ParsedArgs } from '../../types.js';
import { c, dim } from '../../../utils/colors.js';
import { EXIT } from '../../exit-codes.js';
import {
  getAuthStatusAsync,
  refreshAuthToken,
} from '../../../features/github-oauth.js';
import { Spinner } from '../../../utils/spinner.js';
import { formatAuthStatusAsJson, printAuthStatus } from '../shared.js';
import { runAuthMenu } from './menu.js';
import { loginCommand } from './login-command.js';
import { logoutCommand } from './logout-command.js';

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
