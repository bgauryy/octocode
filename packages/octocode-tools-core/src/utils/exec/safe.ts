import { validateCommand } from '@octocodeai/octocode-engine/commandValidator';
import { spawnWithTimeout, validateArgs } from './spawn/wrappers.js';
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
    tolerateOutputLimit = false,
  } = options;

  const result = await spawnWithTimeout(command, args, {
    timeout,
    cwd,
    env,
    maxOutputSize,
  });

  // An output overflow leaves the partial prefix in result.stdout. When the
  // caller opts in, hand that back flagged `truncated` instead of throwing —
  // a bounded prefix is still useful (e.g. `strings` on a huge binary).
  if (result.outputLimitExceeded && tolerateOutputLimit) {
    return {
      success: true,
      code: result.exitCode,
      stdout: result.stdout,
      stderr: result.stderr,
      truncated: true,
    };
  }

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
