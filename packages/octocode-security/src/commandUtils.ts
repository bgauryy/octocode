export function normalizeCommandName(command: string): string {
  if (!command || typeof command !== 'string') return command;
  const lastSep = Math.max(command.lastIndexOf('/'), command.lastIndexOf('\\'));
  const base = lastSep >= 0 ? command.slice(lastSep + 1) : command;
  return base.replace(/\.exe$/i, '');
}
