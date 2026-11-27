import * as vscode from 'vscode';

export const GITHUB_AUTH_PROVIDER_ID = 'github';
const GITHUB_SCOPES = ['repo', 'read:user'];

export type GitHubAuthStatus = {
  authenticated: boolean;
  accountName?: string;
  tokenSource: 'oauth' | 'manual' | 'none';
};

/** The VS Code GitHub session with the scopes Octocode needs. */
export function getGitHubSession(
  options: vscode.AuthenticationGetSessionOptions
): Thenable<vscode.AuthenticationSession | undefined> {
  return vscode.authentication.getSession(
    GITHUB_AUTH_PROVIDER_ID,
    GITHUB_SCOPES,
    options
  );
}

function manualToken(): string | undefined {
  const config = vscode.workspace.getConfiguration('octocode');
  return config.get<string>('githubToken');
}

/** The OAuth session token, else the `octocode.githubToken` setting. */
export async function getGitHubToken(
  log: (line: string) => void
): Promise<string | undefined> {
  try {
    const session = await getGitHubSession({ silent: true });
    if (session) {
      return session.accessToken;
    }
  } catch (err) {
    log(`Error checking GitHub session: ${String(err)}`);
  }

  return manualToken();
}

export async function checkGitHubAuthStatus(
  log: (line: string) => void
): Promise<GitHubAuthStatus> {
  try {
    const session = await getGitHubSession({ silent: true });
    if (session) {
      return {
        authenticated: true,
        accountName: session.account.label,
        tokenSource: 'oauth',
      };
    }
  } catch (err) {
    log(`Error checking GitHub session: ${String(err)}`);
  }

  if (manualToken()) {
    return {
      authenticated: true,
      tokenSource: 'manual',
    };
  }

  return {
    authenticated: false,
    tokenSource: 'none',
  };
}
