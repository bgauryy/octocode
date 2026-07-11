import { c, dim } from '../../../utils/colors.js';
import { EXIT } from '../../exit-codes.js';
import {
  GH_CLI_URL,
  checkGitHubAuth,
  runGitHubAuthLogin,
  runGitHubAuthLogout,
} from '../../../features/gh-auth.js';

export async function runGhLogin(
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

export async function runGhLogout(hostname: string): Promise<void> {
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
