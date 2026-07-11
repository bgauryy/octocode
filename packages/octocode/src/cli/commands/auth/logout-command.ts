import type { CLICommand, ParsedArgs } from '../../types.js';
import { c, bold, dim } from '../../../utils/colors.js';
import { EXIT } from '../../exit-codes.js';
import {
  logout as oauthLogout,
  getAuthStatusAsync,
} from '../../../features/github-oauth.js';
import { printLoginHint } from '../shared.js';

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
      const { confirm } = await import('../../../utils/prompts.js');
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
