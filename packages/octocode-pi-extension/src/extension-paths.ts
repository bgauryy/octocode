import path from 'node:path';
import { workspaceAgentKey } from '@octocodeai/agent-contracts/paths';
import { getOctocodeHome } from '@octocodeai/config';

/** Canonical root for every filesystem artifact owned by the Pi extension. */
export function extensionHome(
  octocodeHome = getOctocodeHome(),
): string {
  return path.join(path.resolve(octocodeHome), 'extension');
}

/** Workspace-scoped extension state without writing into the user repository. */
export function extensionWorkspaceRoot(
  cwd: string,
  octocodeHome = getOctocodeHome(),
): string {
  return path.join(extensionHome(octocodeHome), 'workspaces', workspaceAgentKey(cwd));
}

export function extensionTmpRoot(octocodeHome = getOctocodeHome()): string {
  return path.join(extensionHome(octocodeHome), 'tmp');
}

export function extensionCacheRoot(octocodeHome = getOctocodeHome()): string {
  return path.join(extensionHome(octocodeHome), 'cache');
}

/** Extension-private SQLite state for MCP, skill, catalog, and worker metadata. */
export function extensionStateDbPath(octocodeHome = getOctocodeHome()): string {
  return path.join(extensionHome(octocodeHome), 'state', 'extension.sqlite3');
}
