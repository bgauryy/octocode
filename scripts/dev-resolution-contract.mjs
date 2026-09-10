import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const extensionPackage = JSON.parse(readFileSync(new URL('../packages/octocode-extension-rust/package.json', import.meta.url), 'utf8'));

export const OCTOCODE_CORE_PACKAGE = '@octocodeai/octocode-core';
export const SHARED_PACKAGE = '@octocodeai/agent-contracts';
export const AGENT_TESTING_PACKAGE = '@octocodeai/agent-testing';

export function enginePlatformPackages(enginePackage) {
  return Object.keys(enginePackage.optionalDependencies ?? {}).filter(name =>
    name.startsWith('@octocodeai/octocode-engine-')
  );
}

export function workspaceResolutionPackages(enginePackage) {
  return [
    '@octocodeai/octocode-awareness',
    '@octocodeai/octocode-tools-core',
    '@octocodeai/config',
    '@octocodeai/agent-contracts',
    '@octocodeai/octocode-engine',
    '@octocodeai/octocode-extension-rust',
    ...enginePlatformPackages(enginePackage),
    ...Object.keys(extensionPackage.optionalDependencies ?? {}),
  ];
}

export function managedResolutionPackages(enginePackage) {
  return [
    ...workspaceResolutionPackages(enginePackage),
    OCTOCODE_CORE_PACKAGE,
    AGENT_TESTING_PACKAGE,
    SHARED_PACKAGE,
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

export function localSharedResolution(_repoRoot) {
  // @octocodeai/agent-contracts is now a local workspace package — never resolve from sibling repo.
  return undefined;
}

export function isLocalResolution(spec) {
  return (
    typeof spec === 'string' && /^(?:workspace:|file:|link:|portal:)/.test(spec)
  );
}
