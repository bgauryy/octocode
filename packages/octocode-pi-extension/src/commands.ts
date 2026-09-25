import type { PiCommand, PiInstance } from './types.js';

/** The extension's user command surface; implementations register these contracts. */
export const EXTENSION_COMMANDS = {
  inbox: {
    name: 'octocode-inbox',
    description: 'Inspect, steer, or stop spawned Octocode workers.',
  },
  status: {
    name: 'octocode-status',
    description:
      'Inspect session state; use events for history or export for JSONL.',
  },
  configuration: {
    name: 'configuration',
    description: 'Open Octocode configuration in your local browser.',
  },
  config: {
    name: 'config',
    description: 'Open Octocode configuration in your default browser.',
  },
} as const;

/** Read the current host command registry, hiding internal trampoline commands. */
export function collectPublicCommands(
  pi: Pick<PiInstance, 'getCommands'>
): PiCommand[] {
  let registered: PiCommand[];
  try {
    registered = pi.getCommands?.() ?? [];
  } catch {
    registered = [];
  }

  const byName = new Map<string, PiCommand>();
  for (const command of registered) {
    const name = command.name.trim();
    if (!name || name.startsWith('_')) continue;
    byName.set(name, { ...command, name });
  }
  return [...byName.values()].sort((left, right) =>
    left.name.localeCompare(right.name)
  );
}
