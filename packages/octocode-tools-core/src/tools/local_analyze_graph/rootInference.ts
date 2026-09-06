import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';

/** Prefer the nearest Cargo manifest for Rust; wider workspace scans need an explicit path. */
export function inferRootFromAbsoluteFile(
  absoluteFile: string,
  rustWorkspace: 'syntax' | 'cargo' = 'syntax'
): string {
  let dir = dirname(absoluteFile);
  const rust = rustWorkspace === 'cargo' || absoluteFile.endsWith('.rs');
  let packageRoot: string | undefined;
  while (true) {
    if (rust && existsSync(join(dir, 'Cargo.toml'))) return dir;
    if (!packageRoot && existsSync(join(dir, 'package.json'))) {
      if (!rust) return dir;
      packageRoot = dir;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return packageRoot ?? dirname(absoluteFile);
}
