import { existsSync } from 'fs';
import path from 'path';

import { resolveWorkspaceRoot } from 'octocode-security-utils/workspaceRoot';

const WORKSPACE_MARKERS = [
  'package.json',
  'tsconfig.json',
  '.git',
  'Cargo.toml',
  'go.mod',
  'pyproject.toml',
] as const;

const MAX_WORKSPACE_ASCENT = 25;

function isPathInsideRoot(filePath: string, rootPath: string): boolean {
  const relative = path.relative(rootPath, filePath);
  return (
    relative === '' ||
    (!relative.startsWith('..') && !path.isAbsolute(relative))
  );
}

function hasWorkspaceMarker(directory: string): boolean {
  return WORKSPACE_MARKERS.some(marker =>
    existsSync(path.join(directory, marker))
  );
}

export async function findWorkspaceRoot(filePath: string): Promise<string> {
  const absoluteFilePath = path.resolve(filePath);
  const initialDir = path.dirname(absoluteFilePath);

  let currentDir = initialDir;
  for (let depth = 0; depth < MAX_WORKSPACE_ASCENT; depth += 1) {
    if (hasWorkspaceMarker(currentDir)) {
      return currentDir;
    }

    const parentDir = path.dirname(currentDir);
    if (parentDir === currentDir) {
      break;
    }

    currentDir = parentDir;
  }

  return initialDir;
}

export async function resolveWorkspaceRootForFile(
  filePath: string
): Promise<string> {
  const absoluteFilePath = path.resolve(filePath);
  const configuredRoot = resolveWorkspaceRoot();

  // Prefer the *tightest* root: marker-based discovery from the file's own
  // directory. We only fall back to the configured root (typically
  // `process.cwd()` or `$WORKSPACE_ROOT`) when:
  //   • no marker is found anywhere above the file, OR
  //   • the configured root is itself a tighter match (i.e. lives between the
  //     marker-based root and the file).
  // This avoids the failure mode where MCP servers spawned from `$HOME`
  // accidentally treat the entire home directory as the workspace and pull
  // ripgrep into protected macOS Library paths.
  const markerRoot = await findWorkspaceRoot(absoluteFilePath);

  if (
    isPathInsideRoot(absoluteFilePath, configuredRoot) &&
    isPathInsideRoot(configuredRoot, markerRoot) &&
    configuredRoot !== markerRoot
  ) {
    // configuredRoot is a subdirectory of markerRoot — use the tighter one.
    return configuredRoot;
  }

  return markerRoot;
}
