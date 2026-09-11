export type TerminationSignal = 'SIGINT' | 'SIGTERM';

interface TerminationDependencies {
  stderr: Pick<NodeJS.WriteStream, 'isTTY' | 'write'>;
  exit(code: number): never;
}

export function terminationExitCode(signal: TerminationSignal): 130 | 143 {
  return signal === 'SIGINT' ? 130 : 143;
}

export function terminateForSignal(
  signal: TerminationSignal,
  dependencies: TerminationDependencies = {
    stderr: process.stderr,
    exit: code => process.exit(code),
  }
): never {
  if (dependencies.stderr.isTTY) {
    dependencies.stderr.write('\x1B[?25h');
    dependencies.stderr.write('\n  Goodbye! 👋\n');
  }
  return dependencies.exit(terminationExitCode(signal));
}
