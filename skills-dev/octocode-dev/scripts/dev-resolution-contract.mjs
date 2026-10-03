import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const OCTOCODE_CORE_PACKAGE = '@octocodeai/octocode-core';
export const AGENT_TESTING_PACKAGE = '@octocodeai/agent-testing';

export function nativePlatformPackages(nativePackage) {
  return Object.keys(nativePackage.optionalDependencies ?? {}).filter(name =>
    name.startsWith('@octocodeai/octocode-native-')
  );
}

export function workspaceResolutionPackages(nativePackage) {
  return [
    '@octocodeai/octocode-skill-installer',
    '@octocodeai/config',
    '@octocodeai/octocode-native',
    ...nativePlatformPackages(nativePackage),
  ];
}

export function managedResolutionPackages(nativePackage) {
  return [
    ...workspaceResolutionPackages(nativePackage),
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
