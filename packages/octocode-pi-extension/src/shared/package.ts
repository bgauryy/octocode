import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PACKAGE_NAME = '@octocodeai/pi-extension';

interface Manifest {
  name?: string;
  version?: string;
  bin?: string | Record<string, string>;
  [key: string]: unknown;
}

/** The nearest `package.json` named `name` at or above `fromDir` (at most `maxDepth` levels up). */
export function findManifest(fromDir: string, name: string, maxDepth = 6): { dir: string; manifest: Manifest } | undefined {
  let dir = path.resolve(fromDir);
  for (let depth = 0; depth < maxDepth; depth++, dir = path.dirname(dir)) {
    try {
      const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')) as Manifest;
      if (manifest.name === name) return { dir, manifest };
    } catch {
      // No readable manifest at this level.
    }
    if (path.dirname(dir) === dir) break;
  }
  return undefined;
}

/**
 * This package's root (the folder holding its `package.json`), found by walking up from a module. Source and compiled
 * modules sit at the same depth under `src/` and `dist/`, but not every module is at the top of it, so a fixed `..` breaks.
 */
export function packageRoot(from: string = import.meta.url): string {
  const found = findManifest(path.dirname(fileURLToPath(from)), PACKAGE_NAME);
  if (!found) throw new Error(`Cannot locate the ${PACKAGE_NAME} package root from ${from}`);
  return found.dir;
}
