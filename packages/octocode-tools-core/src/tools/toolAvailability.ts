import { getConfigSync } from '@octocodeai/config';
import { STATIC_TOOL_NAMES } from './toolNames.js';

type AvailabilityConfig = {
  local: { enabled: boolean; enableClone: boolean };
  storage: { mode: string };
  tools: { enabled?: string[] | null; disabled?: string[] | null };
};

/** Shared by discovery and executable follow-ups; importing it needs no engine. */
export function getToolAvailability(
  name: string,
  config: AvailabilityConfig = getConfigSync()
): { enabled: boolean; envVar?: string } {
  const clone = name === STATIC_TOOL_NAMES.GITHUB_CLONE_REPO;
  const local = clone || name.startsWith('local') || name.startsWith('lsp');
  let envVar: string | undefined;
  if (local && !config.local.enabled) envVar = 'ENABLE_LOCAL';
  else if (clone && config.storage.mode !== 'persistent')
    envVar = 'OCTOCODE_STORAGE_MODE';
  else if (clone && !config.local.enableClone) envVar = 'ENABLE_CLONE';
  else if (config.tools.enabled?.length) {
    if (!config.tools.enabled.includes(name)) envVar = 'TOOLS_TO_RUN';
  } else if (config.tools.disabled?.includes(name)) envVar = 'DISABLE_TOOLS';
  return envVar ? { enabled: false, envVar } : { enabled: true };
}
