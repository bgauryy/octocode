import path from 'path';
import os from 'os';

function normalizePath(p: string): string {
  if (!p) return p;
  const normalized = path.posix.normalize(p.replace(/\\/g, '/'));
  return normalized.length > 1 && normalized.endsWith('/')
    ? normalized.slice(0, -1)
    : normalized;
}

function getRelativeIfChild(child: string, parent: string): string | null {
  if (child === parent) return '.';

  const parentPrefix = parent + '/';
  if (child.startsWith(parentPrefix)) {
    return child.slice(parentPrefix.length);
  }

  return null;
}

const HOME_DIR = normalizePath(os.homedir());

export function redactPath(
  absolutePath: string,
  workspaceRoot?: string
): string {
  if (!absolutePath) return '';

  const normalized = normalizePath(absolutePath);
  const rootSource = workspaceRoot ?? process.cwd();
  const root = normalizePath(rootSource);

  const relative = getRelativeIfChild(normalized, root);
  if (relative !== null) return relative;

  if (HOME_DIR) {
    const homeRelative = getRelativeIfChild(normalized, HOME_DIR);
    if (homeRelative !== null) {
      return homeRelative === '.' ? '~' : '~/' + homeRelative;
    }
  }

  return path.basename(normalized);
}
