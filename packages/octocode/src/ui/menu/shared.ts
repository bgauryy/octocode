import { dim } from '../../utils/colors.js';
import { input } from '../../utils/prompts.js';

export async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}
