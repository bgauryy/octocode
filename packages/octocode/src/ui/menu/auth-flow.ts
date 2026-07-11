import { c, dim } from '../../utils/colors.js';
import { loadInquirer, selectWithCancel } from '../../utils/prompts.js';
import { getAuthStatusAsync } from '../../features/github-oauth.js';
import { runGitHubAuthLogout } from '../../features/gh-auth.js';
import { pressEnterToContinue } from './shared.js';
import { showAuthMenu } from './auth-menu.js';
import {
  displayAuthStatus,
  runLoginFlow,
  runLogoutFlow,
  showGhCliGuidance,
} from './auth-actions.js';

export async function runAuthFlow(): Promise<void> {
  await loadInquirer();
  console.log();

  let inAuthMenu = true;
  while (inAuthMenu) {
    const status = await getAuthStatusAsync();

    displayAuthStatus(status);

    const choice = await showAuthMenu(status);

    switch (choice) {
      case 'login': {
        const success = await runLoginFlow();
        console.log();
        if (success) {
          inAuthMenu = false;
        }
        break;
      }

      case 'gh-guidance':
        await showGhCliGuidance();
        break;

      case 'logout':
        await runLogoutFlow();
        console.log();
        break;

      case 'gh-logout': {
        const confirmGh = await selectWithCancel<'yes' | 'no'>({
          message: 'Sign out of gh CLI?',
          choices: [
            { name: 'Yes, sign out', value: 'yes' },
            { name: 'No, cancel', value: 'no' },
          ],
          theme: {
            prefix: '  ',
            style: {
              highlight: (text: string) => c('red', text),
            },
          },
        });

        if (confirmGh !== 'yes') {
          break;
        }

        console.log();
        console.log(`  ${dim('Opening gh auth logout...')}`);
        console.log();
        const ghResult = runGitHubAuthLogout();
        if (ghResult.success) {
          console.log();
          console.log(`  ${c('green', '✅')} Signed out of gh CLI`);
        } else {
          console.log();
          console.log(`  ${c('yellow', '!')} Sign out was cancelled`);
        }
        console.log();
        await pressEnterToContinue();
        break;
      }

      case 'back':
      default:
        inAuthMenu = false;
        break;
    }
  }
}
