import { c, dim, bold } from '../../../utils/colors.js';
import { select, input } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import type { GitHubAuthMethod } from './types.js';

export async function promptGitHubAuth(): Promise<{
  method: Exclude<GitHubAuthMethod, 'back'>;
  token?: string;
} | null> {
  console.log();
  console.log(`  ${c('blue', 'INFO')} ${bold('GitHub Authentication')}`);
  console.log(`  ${dim('Required for accessing GitHub repositories.')}`);
  console.log();

  const method = await select<GitHubAuthMethod>({
    message: 'How would you like to authenticate with GitHub?',
    choices: [
      {
        name: `${c('green', '●')} gh CLI ${dim('(Recommended)')} - ${dim('Uses existing gh auth')}`,
        value: 'gh-cli' as const,
      },
      {
        name: `${c('yellow', '●')} GITHUB_TOKEN - ${dim('Enter personal access token')}`,
        value: 'token' as const,
      },
      {
        name: `${c('dim', '○')} Skip - ${dim('Configure manually later')}`,
        value: 'skip' as const,
      },
      separatorChoice<{ name: string; value: GitHubAuthMethod }>(),
      {
        name: `${c('dim', '- Back')}`,
        value: 'back' as const,
      },
    ],
    loop: false,
  });

  if (method === 'back') return null;

  if (method === 'gh-cli') {
    console.log();
    console.log(
      `  ${c('cyan', '->')} Make sure gh CLI is installed and authenticated:`
    );
    console.log(`    ${dim('https://cli.github.com/')}`);
    console.log();
    console.log(
      `  ${dim('Run')} ${c('cyan', 'gh auth login')} ${dim('if not already authenticated.')}`
    );
    console.log();
    return { method: 'gh-cli' };
  }

  if (method === 'token') {
    console.log();
    console.log(`  ${dim('Leave empty and press Enter to go back')}`);
    console.log();

    const token = await input({
      message: 'Enter your GitHub personal access token:',
      validate: (value: string) => {
        if (!value.trim()) {
          return true;
        }
        if (value.length < 20) {
          return 'Token appears too short';
        }
        return true;
      },
    });

    if (!token || !token.trim()) {
      return null;
    }

    console.log();
    console.log(`  ${c('yellow', 'WARN')} ${bold('Security Note:')}`);
    console.log(
      `  ${dim('Your token will be saved in the MCP configuration file.')}`
    );
    console.log(
      `  ${dim('Make sure this file is not committed to version control.')}`
    );
    console.log();

    return { method: 'token', token };
  }

  return { method: 'skip' };
}
