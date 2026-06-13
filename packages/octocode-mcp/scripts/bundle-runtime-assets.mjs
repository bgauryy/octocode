#!/usr/bin/env node
/**
 * Copies native runtime assets owned by octocode-mcp into dist/runtime.
 *
 * npm packages must be portable: a package published from macOS should still
 * work for Linux and Windows users. Set OCTOCODE_RUNTIME_PLATFORMS=all for the
 * publish build; use OCTOCODE_RUNTIME_PLATFORMS=native for fast local builds.
 */
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  writeFileSync,
} from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(__dirname, '..');
const distDir = join(packageRoot, 'dist');
const runtimeDir = join(distDir, 'runtime');

const PLATFORM_CONFIGS = [
  {
    platform: 'darwin-arm64',
    securityTriple: 'darwin-arm64',
    minifierTriple: 'darwin-arm64',
  },
  {
    platform: 'darwin-x64',
    securityTriple: 'darwin-x64',
    minifierTriple: 'darwin-x64',
  },
  {
    platform: 'linux-arm64',
    securityTriple: 'linux-arm64-gnu',
    minifierTriple: 'linux-arm64-gnu',
  },
  {
    platform: 'linux-x64',
    securityTriple: 'linux-x64-gnu',
    minifierTriple: 'linux-x64-gnu',
  },
  {
    platform: 'linux-x64-musl',
    securityTriple: 'linux-x64-musl',
    minifierTriple: 'linux-x64-musl',
  },
  {
    platform: 'windows-x64',
    securityTriple: 'win32-x64-msvc',
    minifierTriple: 'win32-x64-msvc',
  },
];

const configByPlatform = new Map(
  PLATFORM_CONFIGS.map(config => [config.platform, config])
);

const selectedConfigs = selectPlatformConfigs();

const copiedAssets = {
  mode: process.env.OCTOCODE_RUNTIME_PLATFORMS || 'native',
  host: {
    platform: process.platform,
    arch: process.arch,
  },
  platforms: selectedConfigs.map(config => config.platform),
  security: [],
  minifier: [],
  rg: [],
};

for (const config of selectedConfigs) {
  copiedAssets.security.push(copySecurityNative(config));
  copiedAssets.minifier.push(copyMinifierNative(config));
  copiedAssets.rg.push(bundleRipgrep(config.platform));
}

writeFileSync(
  join(distDir, 'runtime-assets.json'),
  `${JSON.stringify(copiedAssets, null, 2)}\n`
);

console.log(
  `✓ bundled octocode-mcp runtime assets for ${selectedConfigs
    .map(config => config.platform)
    .join(', ')}`
);

function selectPlatformConfigs() {
  const mode = process.env.OCTOCODE_RUNTIME_PLATFORMS || 'native';
  if (mode === 'all') {
    return PLATFORM_CONFIGS;
  }
  if (mode === 'native') {
    return [currentPlatformConfig()];
  }

  const requested = mode
    .split(',')
    .map(value => value.trim())
    .filter(Boolean);

  if (requested.length === 0) {
    throw new Error(
      'OCTOCODE_RUNTIME_PLATFORMS must be "all", "native", or a comma-separated platform list.'
    );
  }

  return requested.map(platform => {
    const config = configByPlatform.get(platform);
    if (!config) {
      throw new Error(
        `Unsupported runtime asset platform: ${platform}. ` +
          `Supported: ${PLATFORM_CONFIGS.map(c => c.platform).join(', ')}`
      );
    }
    return config;
  });
}

function currentPlatformConfig() {
  const platform = currentPlatformKey();
  const config = configByPlatform.get(platform);
  if (!config) {
    throw new Error(`Unsupported runtime asset platform: ${platform}`);
  }
  return config;
}

function currentPlatformKey() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === 'darwin' && arch === 'arm64') return 'darwin-arm64';
  if (platform === 'darwin' && arch === 'x64') return 'darwin-x64';
  if (platform === 'win32' && arch === 'x64') return 'windows-x64';
  if (platform === 'linux' && arch === 'x64') {
    return isMusl() ? 'linux-x64-musl' : 'linux-x64';
  }
  if (platform === 'linux' && arch === 'arm64') {
    if (isMusl()) {
      throw new Error(
        'linux-arm64-musl runtime assets are not built yet. ' +
          'Use OCTOCODE_RUNTIME_PLATFORMS=all on a publish build host with supported artifacts.'
      );
    }
    return 'linux-arm64';
  }
  throw new Error(`Unsupported runtime asset platform: ${platform}-${arch}`);
}

function isMusl() {
  try {
    const { glibcVersionRuntime } = process.report?.getReport()?.header ?? {};
    return !glibcVersionRuntime;
  } catch {
    return true;
  }
}

function copySecurityNative(config) {
  return copyNativeAsset({
    packageName: 'octocode-security',
    binaryName: `octocode-security.${config.securityTriple}.node`,
    runtimeSubdir: 'security',
    platform: config.platform,
    buildHint:
      'Build packages/octocode-security for all publish targets before packages/octocode-mcp.',
  });
}

function copyMinifierNative(config) {
  return copyNativeAsset({
    packageName: 'octocode-minifier-utils',
    binaryName: `octocode-minifier-utils.${config.minifierTriple}.node`,
    runtimeSubdir: 'minifier',
    platform: config.platform,
    buildHint:
      'Build packages/octocode-minifier-utils for all publish targets before packages/octocode-mcp.',
  });
}

function copyNativeAsset({
  packageName,
  binaryName,
  runtimeSubdir,
  platform,
  buildHint,
}) {
  const source = join(packageRoot, '..', packageName, binaryName);
  const destination = join(runtimeDir, runtimeSubdir, binaryName);

  if (!existsSync(source)) {
    throw new Error(
      `Missing ${packageName} native binary for ${platform}: ${source}. ` +
        buildHint
    );
  }

  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  chmodSync(destination, 0o755);

  return {
    platform,
    file: relative(distDir, destination),
  };
}

function bundleRipgrep(platform) {
  const result = spawnSync(
    process.execPath,
    ['scripts/bundle-rg.mjs', platform, distDir, '--runtime-only'],
    {
      cwd: packageRoot,
      stdio: 'inherit',
    }
  );

  if (result.status !== 0) {
    throw new Error(
      `Failed to bundle ripgrep for ${platform}; bundle-rg exited with ${result.status ?? 'unknown status'}.`
    );
  }

  const ext = platform === 'windows-x64' ? '.exe' : '';
  return {
    platform,
    file: relative(distDir, join(runtimeDir, 'rg', `rg-${platform}${ext}`)),
  };
}
