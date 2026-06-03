import type { GitHubAuthStatus } from '../types/index.js';
import {
  runCommand,
  commandExists,
  runInteractiveCommand,
} from '../utils/shell.js';

export const GH_CLI_URL = 'https://cli.github.com/';

export function isGitHubCLIInstalled(): boolean {
  return commandExists('gh');
}

export function checkGitHubAuth(): GitHubAuthStatus {
  if (!isGitHubCLIInstalled()) {
    return {
      installed: false,
      authenticated: false,
      error: 'GitHub CLI (gh) is not installed',
    };
  }

  const result = runCommand('gh', ['auth', 'status']);

  if (result.success) {
    const usernameMatch = result.stdout.match(
      /Logged in to \S+.*account\s+(\S+)/i
    );
    const username = usernameMatch ? usernameMatch[1] : undefined;

    return {
      installed: true,
      authenticated: true,
      username,
    };
  }

  return {
    installed: true,
    authenticated: false,
    error: result.stderr || 'Not authenticated',
  };
}

export function getGitHubCLIVersion(): string | null {
  const result = runCommand('gh', ['--version']);
  if (result.success) {
    const match = result.stdout.match(/gh version ([\d.]+)/);
    return match ? match[1] : result.stdout.split('\n')[0];
  }
  return null;
}

export function getAuthLoginCommand(): string {
  return 'gh auth login';
}

interface GitHubAuthLoginOptions {
  web?: boolean;

  hostname?: string;

  gitProtocol?: 'ssh' | 'https';

  skipSshKey?: boolean;
}

interface GitHubAuthResult {
  success: boolean;
  exitCode: number | null;
}

export function runGitHubAuthLogin(
  options?: GitHubAuthLoginOptions
): GitHubAuthResult {
  const args = ['auth', 'login'];

  if (options?.web) {
    args.push('--web');
  }
  if (options?.hostname) {
    args.push('--hostname', options.hostname);
  }
  if (options?.gitProtocol) {
    args.push('--git-protocol', options.gitProtocol);
  }
  if (options?.skipSshKey) {
    args.push('--skip-ssh-key');
  }

  return runInteractiveCommand('gh', args);
}

export function runGitHubAuthLogout(hostname?: string): GitHubAuthResult {
  const args = ['auth', 'logout'];

  if (hostname) {
    args.push('--hostname', hostname);
  }

  return runInteractiveCommand('gh', args);
}
