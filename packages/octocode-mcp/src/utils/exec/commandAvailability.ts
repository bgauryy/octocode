/**
 * Command availability checking utilities
 * Verifies that required CLI tools (rg, find, ls) are available before use.
 *
 * Note: `grep` is no longer in the required-command set. The MCP ships its
 * own ripgrep via `@vscode/ripgrep`, so the grep fallback that used to live
 * in `searchContentRipgrep` has been removed entirely. POSIX commands that
 * remain here (find, ls) are still required by other tools.
 */

import { spawnCheckSuccess } from './spawn.js';
import {
  resolveRipgrepBinary,
  RIPGREP_PATH_FALLBACK,
} from './ripgrepBinary.js';

/**
 * Result of command availability check
 */
interface CommandAvailabilityResult {
  available: boolean;
  command: string;
  version?: string;
  error?: string;
}

/**
 * Cached availability results to avoid repeated checks
 */
const availabilityCache = new Map<string, CommandAvailabilityResult>();

/**
 * POSIX-standard commands present on macOS/Linux — skip subprocess checks.
 * On Windows these are not assumed and we fall through to spawn checks.
 */
const POSIX_COMMANDS = new Set<string>(['find', 'ls']);

/** Timeout for command availability checks, configurable via environment variable */
const COMMAND_CHECK_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS || '5000', 10) || 5000;

/**
 * Required commands for local tools.
 */
export const REQUIRED_COMMANDS = {
  rg: { name: 'ripgrep', versionFlag: '--version', tool: 'localSearchCode' },
  find: { name: 'find', versionFlag: '--version', tool: 'localFindFiles' },
  ls: { name: 'ls', versionFlag: '--version', tool: 'localViewStructure' },
} as const;

type CommandName = keyof typeof REQUIRED_COMMANDS;

/**
 * Check if a specific command is available.
 * Results are cached for efficiency.
 *
 * @param command - The command to check (rg, find, ls)
 * @param forceCheck - Skip cache and re-check availability
 */
export async function checkCommandAvailability(
  command: CommandName,
  forceCheck = false
): Promise<CommandAvailabilityResult> {
  if (!forceCheck && availabilityCache.has(command)) {
    return availabilityCache.get(command)!;
  }

  const cmdInfo = REQUIRED_COMMANDS[command];

  // POSIX-standard commands are always present on macOS/Linux — skip spawn check.
  if (POSIX_COMMANDS.has(command) && process.platform !== 'win32') {
    const result: CommandAvailabilityResult = {
      available: true,
      command,
    };
    availabilityCache.set(command, result);
    return result;
  }

  try {
    let isAvailable: boolean;

    if (command === 'find' && process.platform === 'darwin') {
      // macOS BSD find doesn't support --version; probe with a no-op invocation.
      isAvailable = await spawnCheckSuccess(
        'find',
        ['.', '-maxdepth', '0'],
        COMMAND_CHECK_TIMEOUT_MS
      );
    } else if (command === 'ls') {
      // ls --version is GNU-only; probe with a basic invocation that works on BSD too.
      isAvailable = await spawnCheckSuccess(
        'ls',
        ['-la', '.'],
        COMMAND_CHECK_TIMEOUT_MS
      );
    } else if (command === 'rg') {
      // Bundled @vscode/ripgrep is preferred. We still spawn-check it because
      // postinstall failures or read-only filesystems can leave the binary
      // unusable; probing the same path the executor will invoke keeps
      // availability honest cross-platform (Windows .exe included).
      const resolved = resolveRipgrepBinary();
      isAvailable = await spawnCheckSuccess(
        resolved === RIPGREP_PATH_FALLBACK ? 'rg' : resolved,
        [cmdInfo.versionFlag],
        COMMAND_CHECK_TIMEOUT_MS
      );
    } else {
      isAvailable = await spawnCheckSuccess(
        command,
        [cmdInfo.versionFlag],
        COMMAND_CHECK_TIMEOUT_MS
      );
    }

    const result: CommandAvailabilityResult = {
      available: isAvailable,
      command,
      ...(isAvailable
        ? {}
        : {
            error: `${cmdInfo.name} (${command}) is not installed or not in PATH`,
          }),
    };

    availabilityCache.set(command, result);
    return result;
  } catch (error) {
    const result: CommandAvailabilityResult = {
      available: false,
      command,
      error:
        error instanceof Error
          ? error.message
          : `Failed to check ${command} availability`,
    };

    availabilityCache.set(command, result);
    return result;
  }
}

/**
 * Check availability of all required commands.
 */
export async function checkAllCommandsAvailability(): Promise<
  Map<CommandName, CommandAvailabilityResult>
> {
  const results = new Map<CommandName, CommandAvailabilityResult>();

  const checks = await Promise.all([
    checkCommandAvailability('rg'),
    checkCommandAvailability('find'),
    checkCommandAvailability('ls'),
  ]);

  results.set('rg', checks[0]!);
  results.set('find', checks[1]!);
  results.set('ls', checks[2]!);

  return results;
}

/**
 * Get a human-readable error message for missing command.
 */
export function getMissingCommandError(command: CommandName): string {
  const cmdInfo = REQUIRED_COMMANDS[command];

  const installInstructions: Record<CommandName, string> = {
    rg: 'Bundled ripgrep failed to load. Reinstall the MCP package (npm i / yarn install) to repair @vscode/ripgrep, or install system ripgrep: brew install ripgrep (macOS), apt install ripgrep (Ubuntu).',
    find: 'find should be available on all Unix systems; on Windows install Git Bash or WSL.',
    ls: 'ls should be available on all Unix systems; on Windows install Git Bash or WSL.',
  };

  return `${cmdInfo.name} (${command}) is not available. ${installInstructions[command]}`;
}

/**
 * Clear the availability cache.
 * @internal Used primarily for testing - not part of public API
 */
export function clearAvailabilityCache(): void {
  availabilityCache.clear();
}
