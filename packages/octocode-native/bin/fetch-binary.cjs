'use strict';

/**
 * Fallback for a skipped platform package (`--omit=optional`, a lockfile written
 * on another platform): download this platform's package from the npm registry
 * the user configured, verify the registry's sha512 integrity, and keep its
 * binary under `<home>/native/<version>/<suffix>/`. It runs at first launch, not
 * in a postinstall script, so it also works where install scripts are disabled
 * (pnpm 10, `--ignore-scripts`). The trust is that of `npm install`: registry
 * metadata over HTTPS and the integrity hash it publishes.
 */

const { createHash } = require('crypto');
const { execFileSync } = require('child_process');
const { existsSync, mkdirSync, renameSync, rmSync, writeFileSync } = require('fs');
const { join } = require('path');
const { gunzipSync } = require('zlib');
const { PLATFORMS, executableName, platformPackageName } = require('./platform.cjs');

const DEFAULT_REGISTRY = 'https://registry.npmjs.org/';
const METADATA_TIMEOUT_MS = 30_000;
const TARBALL_TIMEOUT_MS = 300_000;

/** The registry npm would use for the package's scope, from npm's exported config. */
function registryFor(name, env) {
  const scope = name.startsWith('@') ? name.slice(0, name.indexOf('/')) : null;
  const configured =
    (scope && env[`npm_config_${scope}:registry`]?.trim()) || env.npm_config_registry?.trim() || DEFAULT_REGISTRY;
  return configured.endsWith('/') ? configured : `${configured}/`;
}

async function get(fetchImpl, url, timeoutMs) {
  const response = await fetchImpl(url, { signal: AbortSignal.timeout(timeoutMs), redirect: 'follow' });
  if (!response.ok) throw new Error(`GET ${url} answered HTTP ${response.status}`);
  return response;
}

function verifyIntegrity(bytes, integrity, url) {
  const expected = String(integrity ?? '')
    .split(/\s+/)
    .find(entry => entry.startsWith('sha512-'));
  if (!expected) throw new Error(`registry metadata for ${url} has no sha512 integrity`);
  const actual = `sha512-${createHash('sha512').update(bytes).digest('base64')}`;
  if (actual !== expected) throw new Error(`integrity mismatch for ${url}: expected ${expected}, got ${actual}`);
}

function cString(block, start, end) {
  const field = block.subarray(start, end);
  const nul = field.indexOf(0);
  return field.subarray(0, nul === -1 ? field.length : nul).toString('utf8');
}

/** The `path` record of a pax extended header, when present. */
function paxPath(data) {
  for (const record of data.toString('utf8').split('\n')) {
    const match = /^\d+ path=(.*)$/.exec(record);
    if (match) return match[1];
  }
  return null;
}

/** One regular file from an uncompressed tar archive, or null when absent. */
function extractTarFile(tar, wanted) {
  let offset = 0;
  let override = null;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every(byte => byte === 0)) return null;
    const size = Number.parseInt(cString(header, 124, 136).trim() || '0', 8);
    const type = String.fromCharCode(header[156] || 0x30);
    const prefix = cString(header, 345, 500);
    const name = override ?? (prefix ? `${prefix}/${cString(header, 0, 100)}` : cString(header, 0, 100));
    const data = tar.subarray(offset + 512, offset + 512 + size);
    offset += 512 + Math.ceil(size / 512) * 512;
    if (type === 'x') {
      override = paxPath(data);
      continue;
    }
    override = null;
    if ((type === '0' || type === '\0') && name === wanted) return data;
  }
  return null;
}

/**
 * Path of the cached native binary for `suffix` at `version`, downloading it
 * first when absent. Throws with the reason when any step fails.
 *
 * @param {{ suffix: string, version: string, home: string, env?: NodeJS.ProcessEnv,
 *   fetch?: typeof fetch, log?: (line: string) => void, run?: (bin: string) => string }} options
 * @returns {Promise<string>}
 */
async function fetchNativeBinary({
  suffix,
  version,
  home,
  env = process.env,
  fetch: fetchImpl = fetch,
  log = line => process.stderr.write(`${line}\n`),
  run = bin => execFileSync(bin, ['--version'], { encoding: 'utf8', timeout: 30_000 }),
}) {
  const target = PLATFORMS[suffix];
  if (!target) throw new Error(`no platform package for '${suffix}'`);
  const name = platformPackageName(suffix);
  const file = executableName('octocode', target.os);
  const dir = join(home, 'native', version, suffix);
  const destination = join(dir, file);
  if (existsSync(destination)) return destination;

  const metadataUrl = `${registryFor(name, env)}${name.replace('/', '%2f')}/${encodeURIComponent(version)}`;
  log(`octocode: ${name} is not installed; downloading ${name}@${version} from ${metadataUrl}`);
  const metadata = await (await get(fetchImpl, metadataUrl, METADATA_TIMEOUT_MS)).json();
  const tarballUrl = metadata?.dist?.tarball;
  if (typeof tarballUrl !== 'string') throw new Error(`registry metadata for ${metadataUrl} has no tarball`);
  const tarball = Buffer.from(await (await get(fetchImpl, tarballUrl, TARBALL_TIMEOUT_MS)).arrayBuffer());
  verifyIntegrity(tarball, metadata.dist.integrity, tarballUrl);

  const binary = extractTarFile(gunzipSync(tarball), `package/${file}`);
  if (!binary) throw new Error(`${tarballUrl} holds no package/${file}`);

  mkdirSync(dir, { recursive: true });
  const staged = join(dir, `.${file}.${process.pid}.${Date.now()}.tmp`);
  try {
    writeFileSync(staged, binary, { mode: 0o755 });
    const reported = run(staged).trim();
    if (reported !== `octocode ${version}`) {
      throw new Error(`downloaded binary reports '${reported}', expected 'octocode ${version}'`);
    }
    try {
      renameSync(staged, destination);
    } catch (error) {
      // A concurrent launch published the same verified binary first.
      if (!existsSync(destination)) throw error;
    }
  } finally {
    rmSync(staged, { force: true });
  }
  return destination;
}

module.exports = { extractTarFile, fetchNativeBinary, registryFor };
