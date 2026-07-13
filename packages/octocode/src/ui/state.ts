import {
  getAllClientInstallStatus,
  type ClientInstallStatus,
} from '../utils/mcp-config.js';
import { detectCurrentClient } from '../utils/mcp-paths.js';
import { getAuthStatusAsync } from '../features/github-oauth.js';
import type { OctocodeAuthStatus } from '../types/index.js';

interface OctocodeState {
  installedClients: ClientInstallStatus[];
  availableClients: ClientInstallStatus[];

  installedCount: number;

  availableCount: number;
  isInstalled: boolean;
  hasMoreToInstall: boolean;
}

export interface AppState {
  octocode: OctocodeState;
  currentClient: string | null;
  githubAuth: OctocodeAuthStatus;
}

function getOctocodeState(): OctocodeState {
  const allClients = getAllClientInstallStatus();
  const installedClients = allClients.filter(c => c.octocodeInstalled);
  const availableClients = allClients.filter(
    c => c.configExists && !c.octocodeInstalled
  );

  return {
    installedClients,
    availableClients,
    installedCount: installedClients.length,
    availableCount: availableClients.length,
    isInstalled: installedClients.length > 0,
    hasMoreToInstall: availableClients.length > 0,
  };
}

export async function getAppState(): Promise<AppState> {
  return {
    octocode: getOctocodeState(),
    currentClient: detectCurrentClient(),
    githubAuth: await getAuthStatusAsync(),
  };
}
