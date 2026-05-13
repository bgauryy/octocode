/**
 * Safe command execution with security validation
 * Validates commands and execution context before spawning processes
 */

import { validateCommand } from 'octocode-security-utils/commandValidator';
import { validateExecutionContext } from 'octocode-security-utils/executionContextValidator';
import { spawnWithTimeout, validateArgs } from './spawn.js';
import type { ExecResult, ExecOptions } from '../core/types.js';

/**
 * Safely execute a command with security validation
 * @param command - The command to execute
 * @param args - Command arguments
 * @param options - Execution options (cwd, timeout, maxOutputSize, etc.)
 * @returns Promise resolving to ExecResult
 */
export async function safeExec(
  command: string,
  args: string[] = [],
  options: ExecOptions = {}
): Promise<ExecResult> {
  // Validate command
  const commandValidation = validateCommand(command, args);
  if (!commandValidation.isValid) {
    throw new Error(
      `Command validation failed: ${commandValidation.error || 'Command not allowed'}`
    );
  }

  // Validate arguments (null bytes, length limits)
  const argsValidation = validateArgs(args);
  if (!argsValidation.valid) {
    throw new Error(
      `Argument validation failed: ${argsValidation.error || 'Invalid arguments'}`
    );
  }

  // Validate execution context
  const contextValidation = validateExecutionContext(
    options.cwd,
    process.env.WORKSPACE_ROOT
  );
  if (!contextValidation.isValid) {
    throw new Error(
      `Execution context validation failed: ${contextValidation.error || 'Invalid working directory'}`
    );
  }

  const {
    timeout = 30000,
    cwd,
    env,
    maxOutputSize = 10 * 1024 * 1024, // 10MB default
  } = options;

  const result = await spawnWithTimeout(command, args, {
    timeout,
    cwd,
    env,
    maxOutputSize,
  });

  // Convert SpawnResult to ExecResult format
  // Note: spawnWithTimeout resolves (doesn't reject), so we need to throw on errors
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
