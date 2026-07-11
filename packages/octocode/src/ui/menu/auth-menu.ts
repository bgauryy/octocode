import { c, dim } from '../../utils/colors.js';
import { selectWithCancel } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { checkGitHubAuth } from '../../features/gh-auth.js';
import {
  getGhCliToken,
  getCredentials,
  hasEnvToken,
} from '../../utils/token-storage.js';
import type { OctocodeAuthStatus } from '../../types/index.js';
import type { AuthMenuChoice } from './types.js';

export async function showAuthMenu(
  status: OctocodeAuthStatus
): Promise<AuthMenuChoice> {
  const choices: Array<{
    name: string;
    value: AuthMenuChoice;
    description?: string;
  }> = [];

  const isUsingEnv = status.tokenSource === 'env';

  const ghCliToken = await getGhCliToken();
  const ghAuth = checkGitHubAuth();
  const octocodeCredentials = await getCredentials();

  const hasGhCli = !!ghCliToken;
  const hasOctocode = !!octocodeCredentials;
  const hasEnv = hasEnvToken();

  if (isUsingEnv && hasEnv) {
    const envVar =
      status.envTokenSource?.replace('env:', '') || 'environment variable';
    choices.push(
      separatorChoice<{
        name: string;
        value: AuthMenuChoice;
        description?: string;
      }>(
        `  ${c('green', '✅')} Using ${c('cyan', envVar)} ${dim('(takes priority)')}`
      )
    );

    choices.push(
      separatorChoice<{
        name: string;
        value: AuthMenuChoice;
        description?: string;
      }>()
    );
  }

  if (hasGhCli) {
    const userPart = ghAuth.username ? ` (@${ghAuth.username})` : '';
    choices.push({
      name: `- Delete gh CLI token${userPart}`,
      value: 'gh-logout',
      description: 'Opens gh auth logout',
    });
  }

  if (hasOctocode) {
    const userPart = octocodeCredentials.username
      ? ` (@${octocodeCredentials.username})`
      : '';
    const storageType = 'file';
    choices.push({
      name: `- Delete Octocode token${userPart}`,
      value: 'logout',
      description: `Remove from ${storageType}`,
    });
  }

  if (!hasOctocode) {
    choices.push({
      name: `- Sign In via Octocode ${c('green', '(Recommended)')}`,
      value: 'login',
      description: 'Quick browser sign in',
    });
  }

  if (!hasGhCli) {
    choices.push({
      name: '- Sign In via gh CLI',
      value: 'gh-guidance',
      description: ghAuth.installed
        ? 'Use existing GitHub CLI'
        : 'GitHub CLI not installed',
    });
  }

  choices.push(
    separatorChoice<{
      name: string;
      value: AuthMenuChoice;
      description?: string;
    }>()
  );

  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const choice = await selectWithCancel<AuthMenuChoice>({
    message: '',
    choices,
    pageSize: 12,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}
