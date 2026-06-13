import { existsSync } from 'fs';
import path from 'path';

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
  const configuredRoot = process.cwd();

  const markerRoot = await findWorkspaceRoot(absoluteFilePath);

  if (
    isPathInsideRoot(absoluteFilePath, configuredRoot) &&
    isPathInsideRoot(configuredRoot, markerRoot) &&
    configuredRoot !== markerRoot
  ) {
    return configuredRoot;
  }

  return markerRoot;
}
