import { createRequire } from 'node:module';

const { PLATFORMS, platformPackageName } = createRequire(import.meta.url)(
  '../../../packages/octocode-native/bin/platform.cjs'
);

export function managedResolutionPackages() {
  return [
    '@octocodeai/config',
    '@octocodeai/octocode-core',
    '@octocodeai/agent-testing',
    ...Object.keys(PLATFORMS).map(platformPackageName),
  ];
}

export function isLocalResolution(spec) {
  return (
    typeof spec === 'string' && /^(?:workspace:|file:|link:|portal:)/.test(spec)
  );
}
