import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { RuntimeFailure } from '@octocodeai/agent-core';

const PI_HOST_PACKAGE = '@earendil-works/pi-coding-agent';
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

export const APPROVED_PI_HOST_VERSION = '0.84.4' as const;

export class PiHostCompatibilityError extends RuntimeFailure {
  readonly code = 'OCTOCODE_PI_HOST_INCOMPATIBLE' as const;

  constructor(
    readonly expectedVersion: string,
    readonly actualVersion: string | undefined,
  ) {
    super('adapter-compatibility',
      actualVersion
        ? `Unsupported Pi host ${actualVersion}; Octocode requires exactly ${expectedVersion}`
        : `Unable to verify the Pi host version; Octocode requires exactly ${expectedVersion}`,
      'unsafe',
      true,
      'public',
      'runtime',
    );
    this.name = 'PiHostCompatibilityError';
  }
}

function normalizeVersion(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  const normalized = value.trim().replace(/^v/, '');
  return SEMVER.test(normalized) ? normalized : undefined;
}

function readPackageVersion(packageJsonPath: string): string | undefined {
  try {
    const parsed = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8')) as { name?: unknown; version?: unknown };
    if (parsed.name !== PI_HOST_PACKAGE) return undefined;
    return normalizeVersion(parsed.version);
  } catch {
    return undefined;
  }
}

function findPeerManifestVersion(start: string): string | undefined {
  let current = path.resolve(start);
  for (let depth = 0; depth < 12; depth += 1) {
    const version = readPackageVersion(path.join(current, 'node_modules', ...PI_HOST_PACKAGE.split('/'), 'package.json'));
    if (version) return version;
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return undefined;
}

export function resolveInstalledPiHostVersion(): string | undefined {
  const require = createRequire(import.meta.url);
  try {
    return readPackageVersion(require.resolve(`${PI_HOST_PACKAGE}/package.json`));
  } catch {
    // Some package export maps hide package.json. Resolve the public entry and
    // inspect only its bounded ancestor chain for the owning manifest.
  }

  try {
    let current = path.dirname(require.resolve(PI_HOST_PACKAGE));
    for (let depth = 0; depth < 12; depth += 1) {
      const version = readPackageVersion(path.join(current, 'package.json'));
      if (version) return version;
      const parent = path.dirname(current);
      if (parent === current) break;
      current = parent;
    }
  } catch {
    // Pi publishes types without a runtime root export. In that layout Node
    // cannot resolve the package entry or manifest through the export map, so
    // locate only the exact peer manifest from bounded module/cwd ancestors.
  }
  return findPeerManifestVersion(import.meta.dirname) ?? findPeerManifestVersion(process.cwd());
}

export function resolvePiHostVersion(
  host: unknown,
  resolveInstalled: () => string | undefined = resolveInstalledPiHostVersion,
): string | undefined {
  if (host && typeof host === 'object') {
    const metadata = host as { hostVersion?: unknown; version?: unknown };
    const explicit = normalizeVersion(metadata.hostVersion) ?? normalizeVersion(metadata.version);
    if (explicit) return explicit;
  }
  return resolveInstalled();
}

export function assertSupportedPiHostVersion(version: string | undefined): asserts version is typeof APPROVED_PI_HOST_VERSION {
  if (version !== APPROVED_PI_HOST_VERSION) {
    throw new PiHostCompatibilityError(APPROVED_PI_HOST_VERSION, version);
  }
}
