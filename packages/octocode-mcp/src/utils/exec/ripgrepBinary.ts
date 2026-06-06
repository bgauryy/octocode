let cachedPath: string | null = null;

export function resolveRipgrepBinary(): string {
  if (cachedPath !== null) return cachedPath;
  cachedPath = computePath();
  return cachedPath;
}

function computePath(): string {
  try {
    const mod = require('@vscode/ripgrep') as { rgPath?: string };
    if (mod.rgPath && typeof mod.rgPath === 'string') {
      return mod.rgPath;
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Bundled ripgrep is unavailable: ${message}`);
  }
  throw new Error(
    'Bundled ripgrep is unavailable: @vscode/ripgrep did not export rgPath'
  );
}
