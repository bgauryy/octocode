import { c, dim } from '../../../utils/colors.js';
import { EXIT } from '../../exit-codes.js';
import type { getAuthStatusAsync } from '../../../features/github-oauth.js';
import type { ParsedArgs } from '../../types.js';

export type AuthMenuAction =
  'login' | 'logout' | 'switch' | 'gh-login' | 'gh-logout' | 'back';

export function isOctocodeAuthStatus(
  status: Awaited<ReturnType<typeof getAuthStatusAsync>>
): boolean {
  return status.tokenSource === 'octocode';
}

export function normalizeGitProtocol(
  value: ParsedArgs['options'][string]
): 'ssh' | 'https' | null {
  const gitProtocol = typeof value === 'string' ? value : 'https';
  return gitProtocol === 'ssh' || gitProtocol === 'https' ? gitProtocol : null;
}

export function printInvalidGitProtocol(
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
