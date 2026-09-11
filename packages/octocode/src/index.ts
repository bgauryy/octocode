import { dim } from './utils/colors.js';
import { terminateForSignal } from './cli/process-lifecycle.js';

async function showTopLevelHelp(): Promise<void> {
  const { showHelp } = await import('./cli/main-help.js');
  showHelp();
}

async function main(): Promise<void> {
  const { runCLI } = await import('./cli/index.js');
  const handled = await runCLI();

  if (handled) {
    return;
  }

  await showTopLevelHelp();
}

process.on('SIGINT', () => terminateForSignal('SIGINT'));
process.on('SIGTERM', () => terminateForSignal('SIGTERM'));

function isExitPromptError(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'name' in error &&
    error.name === 'ExitPromptError'
  );
}

function wantsJsonOutput(): boolean {
  return process.argv.slice(2).includes('--json');
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

main().catch((err: unknown) => {
  if (isExitPromptError(err)) {
    console.log();
    console.log(dim('  Goodbye! 👋'));
    process.exit(0);
  }
  if (wantsJsonOutput()) {
    console.log(
      JSON.stringify({
        success: false,
        error: errorMessage(err),
      })
    );
    process.exit(1);
  }
  console.error('Error:', err);
  process.exit(1);
});
