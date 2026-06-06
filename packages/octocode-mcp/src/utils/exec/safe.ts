import { validateCommand } from 'octocode-security-utils/commandValidator';
import { spawnWithTimeout, validateArgs } from './spawn.js';
import type { ExecResult, ExecOptions } from '../core/types.js';

export async function safeExec(
  command: string,
  args: string[] = [],
  options: ExecOptions = {}
): Promise<ExecResult> {
  const commandValidation = validateCommand(command, args);
  if (!commandValidation.isValid) {
    throw new Error(
      `Command validation failed: ${commandValidation.error || 'Command not allowed'}`
    );
  }

  const argsValidation = validateArgs(args);
  if (!argsValidation.valid) {
    throw new Error(
      `Argument validation failed: ${argsValidation.error || 'Invalid arguments'}`
    );
  }

  const {
    timeout = 30000,
    cwd,
    env,
    maxOutputSize = 10 * 1024 * 1024,
  } = options;

  const result = await spawnWithTimeout(command, args, {
    timeout,
    cwd,
    env,
    maxOutputSize,
  });

  if (result.error) {
    throw result.error;
  }

  return {
    success: result.success,
    code: result.exitCode,
    stdout: result.stdout,
    stderr: result.stderr,
  };
}
