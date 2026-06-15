import { spawnCheckSuccess } from './spawn.js';
import { resolveRipgrepBinary } from './ripgrepBinary.js';

interface CommandAvailabilityResult {
  available: boolean;
  command: string;
  version?: string;
  error?: string;
}

const availabilityCache = new Map<string, CommandAvailabilityResult>();

const COMMAND_CHECK_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS || '5000', 10) || 5000;

export const REQUIRED_COMMANDS = {
  rg: { name: 'ripgrep', versionFlag: '--version', tool: 'localSearchCode' },
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

  try {
    const resolved = resolveRipgrepBinary();
    const isAvailable = await spawnCheckSuccess(
      resolved,
      [cmdInfo.versionFlag],
      COMMAND_CHECK_TIMEOUT_MS
    );

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

  const checks = await Promise.all([checkCommandAvailability('rg')]);

  results.set('rg', checks[0]!);

  return results;
}

export function getMissingCommandError(command: CommandName): string {
  const cmdInfo = REQUIRED_COMMANDS[command];

  const installInstructions: Record<CommandName, string> = {
    rg: 'Bundled ripgrep failed to load. Reinstall the MCP package to restore dist/runtime/rg.',
  };

  return `${cmdInfo.name} (${command}) is not available. ${installInstructions[command]}`;
}

export function clearAvailabilityCache(): void {
  availabilityCache.clear();
}
