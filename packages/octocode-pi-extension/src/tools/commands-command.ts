import type { PiCommand, PiInstance } from '../types.js';

export type CommandGuideAuthState =
  | { status: 'checking' }
  | { status: 'authenticated'; source?: string }
  | { status: 'missing' }
  | { status: 'error' };

const MISSING_DESCRIPTION = 'No description provided.';

function isOctocodeCommand(command: PiCommand): boolean {
  return command.name === 'commands' || command.name.startsWith('octocode-');
}

function formatCommand(command: PiCommand): string {
  const description = command.description?.trim() || MISSING_DESCRIPTION;
  return `  /${command.name} — Use when: ${description}`;
}

function formatSection(title: string, commands: PiCommand[]): string[] {
  if (commands.length === 0) return [];
  return [title, ...commands.map(formatCommand), ''];
}

function formatGitHubAuth(state: CommandGuideAuthState | undefined): string[] {
  if (!state) return [];
  if (state.status === 'authenticated') {
    return [`GitHub: authenticated${state.source ? ` via ${state.source}` : ''}`];
  }
  if (state.status === 'checking') return ['GitHub: checking authentication…'];
  if (state.status === 'missing') {
    return [
      'GitHub: login required',
      '  Run: npx octocode auth login',
      '  Or:  gh auth login',
    ];
  }
  return [
    'GitHub: authentication check failed',
    '  Retry: npx octocode auth status --json',
    '  Login: npx octocode auth login or gh auth login',
  ];
}

/** Read the current host command registry, hiding internal trampoline commands. */
export function collectPublicCommands(pi: Pick<PiInstance, 'getCommands'>): PiCommand[] {
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
  return [...byName.values()].sort((left, right) => left.name.localeCompare(right.name));
}

/** Format a readable command reference from the live registry snapshot. */
export function formatCommandsGuide(
  commands: readonly PiCommand[],
  authState?: CommandGuideAuthState,
): string {
  const octocode: PiCommand[] = [];
  const host: PiCommand[] = [];
  const skillsAndTemplates: PiCommand[] = [];

  for (const command of [...commands].sort((left, right) => left.name.localeCompare(right.name))) {
    if (isOctocodeCommand(command)) octocode.push(command);
    else if (command.source === 'skill' || command.source === 'prompt') skillsAndTemplates.push(command);
    else host.push(command);
  }

  const lines = [
    '◆ Commands — live slash-command guide',
    '',
    ...formatSection('Octocode commands', octocode),
    ...formatSection('Pi and extension commands', host),
    ...formatSection('Skills and templates', skillsAndTemplates),
    ...formatGitHubAuth(authState),
  ];
  return lines.join('\n').trimEnd();
}
