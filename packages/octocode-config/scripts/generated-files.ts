import { readFileSync, writeFileSync } from 'node:fs';

/** Publish generated source without invalidating downstream builds for unchanged content. */
export function writeGenerated(path: string, content: string): void {
  if (readGenerated(path) === content) return;
  writeFileSync(path, content);
}

/** Missing output is stale; other filesystem failures must remain visible. */
export function readGenerated(path: string): string | undefined {
  try {
    return readFileSync(path, 'utf8');
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') return undefined;
    throw error;
  }
}
