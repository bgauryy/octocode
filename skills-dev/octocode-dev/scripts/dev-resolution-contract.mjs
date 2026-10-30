import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const OCTOCODE_CORE_PACKAGE = '@octocodeai/octocode-core';
export const AGENT_TESTING_PACKAGE = '@octocodeai/agent-testing';

const { PLATFORMS, platformPackageName } = createRequire(import.meta.url)(
  '../../../packages/octocode-native/bin/platform.cjs'
);

/** The per-platform binary packages, from the one platform list. */
function nativePlatformPackages() {
  return Object.keys(PLATFORMS).map(platformPackageName);
}

export function workspaceResolutionPackages() {
  return [
    '@octocodeai/config',
    ...nativePlatformPackages(),
  ];
}

export function managedResolutionPackages() {
  return [
    ...workspaceResolutionPackages(),
    OCTOCODE_CORE_PACKAGE,
    AGENT_TESTING_PACKAGE,
  ];
}

export function localCoreResolution(repoRoot) {
  const directory = resolve(
    repoRoot,
    '../octocode-mcp-host/packages/octocode-core'
  );
  return existsSync(directory) ? pathToFileURL(directory).href : undefined;
}

export function localAgentTestingResolution(repoRoot) {
  const directory = resolve(
    repoRoot,
    '../octocode-agent/packages/octocode-agent-testing'
  );
  return existsSync(directory) ? pathToFileURL(directory).href : undefined;
}

export function isLocalResolution(spec) {
  return (
    typeof spec === 'string' && /^(?:workspace:|file:|link:|portal:)/.test(spec)
  );
}
