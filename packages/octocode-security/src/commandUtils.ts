/**
 * commandUtils.ts — shared command-name normalisation.
 * Imported by both commandValidator.ts and registry.ts so that extra-allowed
 * commands are stored in the same canonical form that the validator compares
 * against (strip path prefix + .exe suffix).
 */

/**
 * Strips any path prefix and `.exe` suffix so that platform-specific command
 * spellings (`/usr/bin/rg`, `rg.exe`, `rg-darwin-arm64`) all normalise to
 * their base name.
 */
export function normalizeCommandName(command: string): string {
  if (!command || typeof command !== 'string') return command;
  const lastSep = Math.max(command.lastIndexOf('/'), command.lastIndexOf('\\'));
  const base = lastSep >= 0 ? command.slice(lastSep + 1) : command;
  return base.replace(/\.exe$/i, '');
}
