import { dim } from './utils/colors.js';

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

function handleTermination(): void {
  process.stdout.write('\x1B[?25h');
  console.log();
  console.log(dim('  Goodbye! 👋'));
  process.exit(0);
}

process.on('SIGINT', handleTermination);

process.on('SIGTERM', handleTermination);

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
