import { spawnCheckSuccess } from './spawn.js';
import { resolveRipgrepBinary } from './ripgrepBinary.js';

interface CommandAvailabilityResult {
  available: boolean;
  command: string;
  version?: string;
  error?: string;
}

const availabilityCache = new Map<string, CommandAvailabilityResult>();

const POSIX_COMMANDS = new Set<string>(['find', 'ls']);

const COMMAND_CHECK_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS || '5000', 10) || 5000;

export const REQUIRED_COMMANDS = {
  rg: { name: 'ripgrep', versionFlag: '--version', tool: 'localSearchCode' },
  find: { name: 'find', versionFlag: '--version', tool: 'localFindFiles' },
  ls: { name: 'ls', versionFlag: '--version', tool: 'localViewStructure' },
} as const;

type CommandName = keyof typeof REQUIRED_COMMANDS;

export async function checkCommandAvailability(
  command: CommandName,
  forceCheck = false
): Promise<CommandAvailabilityResult> {
  if (!forceCheck && availabilityCache.has(command)) {
    return availabilityCache.get(command)!;
  }

  const cmdInfo = REQUIRED_COMMANDS[command];

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
      isAvailable = await spawnCheckSuccess(
        'find',
        ['.', '-maxdepth', '0'],
        COMMAND_CHECK_TIMEOUT_MS
      );
    } else if (command === 'ls') {
      isAvailable = await spawnCheckSuccess(
        'ls',
        ['-la', '.'],
        COMMAND_CHECK_TIMEOUT_MS
      );
    } else if (command === 'rg') {
      const resolved = resolveRipgrepBinary();
      isAvailable = await spawnCheckSuccess(
        resolved,
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
            error: `${cmdInfo.name} (${command}) bundled binary is unavailable`,
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

export function getMissingCommandError(command: CommandName): string {
  const cmdInfo = REQUIRED_COMMANDS[command];

  const installInstructions: Record<CommandName, string> = {
    rg: 'Bundled ripgrep failed to load. Reinstall the MCP package (npm i / yarn install) to repair @vscode/ripgrep.',
    find: 'find should be available on all Unix systems; on Windows install Git Bash or WSL.',
    ls: 'ls should be available on all Unix systems; on Windows install Git Bash or WSL.',
  };

  return `${cmdInfo.name} (${command}) is not available. ${installInstructions[command]}`;
}

export function clearAvailabilityCache(): void {
  availabilityCache.clear();
}
