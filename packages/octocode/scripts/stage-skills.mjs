import { cpSync, existsSync, lstatSync, rmSync } from 'node:fs';
import { basename } from 'node:path';

const EXCLUDED = new Set([
  '__pycache__', 'coverage', 'dist', 'node_modules', 'out', 'target',
  'Thumbs.db', 'npm-debug.log', 'yarn-error.log',
]);

/** The only skill staging path used by build and prepack. */
export function stageSkills(source, target) {
  if (!existsSync(source)) throw new Error(`Skills source not found: ${source}`);
  rmSync(target, { recursive: true, force: true });
  cpSync(source, target, {
    recursive: true,
    filter: path => {
      const name = basename(path);
      return !name.startsWith('.') && !EXCLUDED.has(name) && !lstatSync(path).isSymbolicLink();
    },
  });
}
