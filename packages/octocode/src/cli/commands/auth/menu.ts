import type { ParsedArgs } from '../../types.js';
import { c, dim } from '../../../utils/colors.js';
import {
  getAuthStatusAsync,
  getStoragePath,
  logout as oauthLogout,
} from '../../../features/github-oauth.js';
import { loadInquirer, select } from '../../../utils/prompts.js';
import { printAuthStatus } from '../shared.js';
import { checkGitHubAuth } from '../../../features/gh-auth.js';
import { getCredentials, hasEnvToken } from '../../../utils/token-storage.js';
import {
  type AuthMenuAction,
  isOctocodeAuthStatus,
  normalizeGitProtocol,
  printInvalidGitProtocol,
} from './helpers.js';
import { runOctocodeLogin } from './login-flow.js';
import { runGhLogin, runGhLogout } from './gh-flow.js';

export async function runAuthMenu(args: ParsedArgs): Promise<void> {
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
