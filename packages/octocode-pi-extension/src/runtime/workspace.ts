import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
export function normalizeWorkspacePath(workspace?: string | null, cwd?: string): string | null {
  const candidate = resolve(workspace || cwd || process.cwd());
  let root = candidate;
  try { root = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: candidate, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 2000 }).trim(); } catch { /* A non-repository workspace keeps its own identity. */ }
  try { return realpathSync(root); } catch { return root; }
}
