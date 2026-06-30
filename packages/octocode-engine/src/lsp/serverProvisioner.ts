import { createHash } from 'node:crypto';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  openSync,
  closeSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { gunzipSync, inflateRawSync } from 'node:zlib';

import { detectPlatformId, type PlatformId } from './platform.js';
import {
  cachedServerBinPath,
  managedCacheRoot,
  manifestServer,
  provisionMode,
  resolveCachedServer,
  type ManifestAsset,
} from './serverManifest.js';

/**
 * The write half of LSP provisioning — downloads a pinned, verified portable
 * server binary into the managed cache. This is invoked ONLY by the explicit
 * `octocode lsp-server install` path, never lazily during a semantic query
 * (the resolution ladder's L4 stays read-only). Provisioning a server is
 * orthogonal to the runtime contract: when no server is available at query
 * time the tool layer throws and the agent pivots to text search.
 *
 * Security invariants (all enforced here):
 *   - opt-in: refuses unless OCTOCODE_LSP_AUTO_INSTALL is prompt|auto
 *   - pinned SHA-256 gate: refuses if the manifest sha256 is null
 *   - host allowlist on the request URL AND every redirect hop; https only
 *   - atomic: download to a temp file in the dest dir, verify, chmod, rename
 *   - `.ok` completion marker written last, so a partial install never resolves
 *   - per-target lock so editor + CLI don't race the same download
 * v1 handles `gz` and `none` (raw) assets with zero new deps; archive formats
 * needing zip/tar/xz extraction return a clear "install manually" error.
 */

const ALLOWED_HOSTS = new Set([
  'github.com',
  // GitHub release assets redirect to a CDN host; both the legacy
  // (objects.) and current (release-assets.) hosts are GitHub-owned.
  'objects.githubusercontent.com',
  'release-assets.githubusercontent.com',
  'releases.hashicorp.com',
]);

const MAX_REDIRECTS = 5;
const LOCK_STALE_MS = 10 * 60 * 1000;

export interface ProvisionResult {
  ok: boolean;
  path?: string;
  source?: 'already-present' | 'downloaded';
  error?: string;
}

function fail(error: string): ProvisionResult {
  return { ok: false, error };
}

function hostAllowed(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'https:' && ALLOWED_HOSTS.has(parsed.hostname);
  } catch {
    return false;
  }
}

/** Fetch following redirects manually, re-checking the host allowlist each hop. */
async function fetchAllowlisted(
  url: string,
  signal?: AbortSignal
): Promise<{ ok: true; body: ArrayBuffer } | { ok: false; error: string }> {
  let current = url;
  for (let hop = 0; hop <= MAX_REDIRECTS; hop++) {
    if (!hostAllowed(current)) {
      // Report only the host — release-asset URLs carry signed query tokens we
      // must not echo into logs/CLI output.
      let host: string;
      try {
        host = new URL(current).host;
      } catch {
        host = '(unparseable url)';
      }
      return { ok: false, error: `Blocked non-allowlisted/insecure host: ${host}` };
    }
    let response: Response;
    try {
      response = await fetch(current, { redirect: 'manual', signal });
    } catch (err) {
      return { ok: false, error: `Download failed: ${err instanceof Error ? err.message : String(err)}` };
    }
    if (response.status >= 300 && response.status < 400) {
      const location = response.headers.get('location');
      if (!location) return { ok: false, error: 'Redirect without Location header' };
      current = new URL(location, current).toString();
      continue;
    }
    if (!response.ok) {
      return { ok: false, error: `Download failed: HTTP ${response.status}` };
    }
    return { ok: true, body: await response.arrayBuffer() };
  }
  return { ok: false, error: 'Too many redirects' };
}

function sha256(buffer: Buffer): string {
  return createHash('sha256').update(buffer).digest('hex');
}

/** Decode the downloaded asset into the final executable bytes. */
function extractBinary(
  asset: ManifestAsset,
  raw: Buffer
): { ok: true; bytes: Buffer } | { ok: false; error: string } {
  switch (asset.archive) {
    case 'none':
      return { ok: true, bytes: raw };
    case 'gz':
      return { ok: true, bytes: gunzipSync(raw) };
    case 'zip': {
      if (!asset.binPath) {
        return { ok: false, error: 'zip asset is missing binPath in manifest' };
      }
      const extracted = extractFromZip(raw, asset.binPath);
      if (!extracted) {
        return {
          ok: false,
          error: `Could not find '${asset.binPath}' in zip archive — try installing via your package manager.`,
        };
      }
      return { ok: true, bytes: extracted };
    }
    case 'tar.gz':
    case 'tar.xz':
      return {
        ok: false,
        error: `Archive format '${asset.archive}' is not supported; install the server via your package manager.`,
      };
    default:
      return { ok: false, error: `Unknown archive format '${asset.archive}'` };
  }
}

/**
 * Minimal ZIP local-file-header parser. Handles stored (method 0) and deflate
 * (method 8) entries — the two methods used by all known LSP server releases.
 * Searches for `targetPath` by exact match or trailing-segment match so both
 * `clangd_22.1.0/bin/clangd` and `./clangd_22.1.0/bin/clangd` resolve.
 */
function extractFromZip(zipBuffer: Buffer, targetPath: string): Buffer | null {
  const LOCAL_HEADER_SIG = 0x04034b50;
  const normalizedTarget = targetPath.replace(/^\.?\//u, '');
  let offset = 0;

  while (offset + 30 < zipBuffer.length) {
    if (zipBuffer.readUInt32LE(offset) !== LOCAL_HEADER_SIG) break;

    const compressionMethod = zipBuffer.readUInt16LE(offset + 8);
    const compressedSize = zipBuffer.readUInt32LE(offset + 18);
    const filenameLen = zipBuffer.readUInt16LE(offset + 26);
    const extraLen = zipBuffer.readUInt16LE(offset + 28);
    const filename = zipBuffer
      .subarray(offset + 30, offset + 30 + filenameLen)
      .toString('utf8')
      .replace(/^\.?\//u, '');

    const dataOffset = offset + 30 + filenameLen + extraLen;
    // Guard against a malformed or truncated ZIP with bogus size fields —
    // compressedSize is UInt32 and could overflow past the buffer.
    if (dataOffset + compressedSize > zipBuffer.length) break;

    if (filename === normalizedTarget) {
      const compressed = zipBuffer.subarray(dataOffset, dataOffset + compressedSize);
      if (compressionMethod === 0) return compressed; // stored
      if (compressionMethod === 8) return inflateRawSync(compressed); // deflate
      return null; // unsupported method within zip
    }

    offset = dataOffset + compressedSize;
  }

  return null;
}

function acquireLock(lockPath: string): boolean {
  try {
    closeSync(openSync(lockPath, 'wx'));
    return true;
  } catch {
    // Reclaim a stale lock from a crashed/killed installer.
    try {
      if (Date.now() - statSync(lockPath).mtimeMs > LOCK_STALE_MS) {
        rmSync(lockPath, { force: true });
        closeSync(openSync(lockPath, 'wx'));
        return true;
      }
    } catch {
      /* fall through */
    }
    return false;
  }
}

/**
 * Provision `serverName` into the managed cache. Idempotent: returns the
 * already-present path when a verified copy exists. Pure-data inputs come from
 * `serverManifest.json`; nothing is executed during install.
 */
export async function provisionServer(
  serverName: string,
  options: { mode?: ReturnType<typeof provisionMode>; signal?: AbortSignal } = {}
): Promise<ProvisionResult> {
  const platformId: PlatformId = detectPlatformId();
  const mode = options.mode ?? provisionMode();

  const server = manifestServer(serverName);
  if (!server) return fail(`${serverName} is not an auto-downloadable server.`);

  const unsupported = server.unsupportedPlatforms?.[platformId];
  if (unsupported) return fail(unsupported);

  const asset = server.platforms[platformId];
  if (!asset) {
    return fail(`No ${serverName} asset for platform ${platformId}.`);
  }

  // Idempotent: a verified copy is already present.
  const existing = resolveCachedServer(serverName, platformId);
  if (existing) return { ok: true, path: existing, source: 'already-present' };

  if (mode === 'off') {
    return fail(
      'Auto-install is off. Set OCTOCODE_LSP_AUTO_INSTALL=prompt|auto (or pass --yes) to allow downloading.'
    );
  }
  if (!asset.sha256) {
    return fail(
      `${serverName} has no pinned sha256 in the manifest yet; refusing to download unverified bytes.`
    );
  }

  const binPath = cachedServerBinPath(serverName, platformId);
  if (!binPath) return fail(`Cannot compute cache path for ${serverName}.`);
  const dir = path.dirname(binPath);
  mkdirSync(dir, { recursive: true });

  const lockPath = path.join(dir, '.lock');
  if (!acquireLock(lockPath)) {
    return fail(`Another install of ${serverName} is in progress (${lockPath}).`);
  }

  try {
    // Re-check after acquiring the lock (another process may have finished).
    const racedWinner = resolveCachedServer(serverName, platformId);
    if (racedWinner) return { ok: true, path: racedWinner, source: 'already-present' };

    const fetched = await fetchAllowlisted(asset.url, options.signal);
    if (!fetched.ok) return fail(fetched.error);

    const downloaded = Buffer.from(fetched.body);
    const actualSha = sha256(downloaded);
    if (actualSha !== asset.sha256) {
      return fail(
        `Checksum mismatch for ${serverName}: expected ${asset.sha256}, got ${actualSha}.`
      );
    }

    const extracted = extractBinary(asset, downloaded);
    if (!extracted.ok) return fail(extracted.error);

    // Atomic: write a temp file in the same dir, chmod, then rename into place.
    const tmpPath = `${binPath}.tmp-${process.pid}`;
    writeFileSync(tmpPath, extracted.bytes);
    if (process.platform !== 'win32') chmodSync(tmpPath, 0o755);
    renameSync(tmpPath, binPath);

    // Completion marker LAST — resolveCachedServer trusts the binary only now.
    writeFileSync(
      `${binPath}.ok`,
      JSON.stringify({ sha256: asset.sha256, size: extracted.bytes.length })
    );

    return { ok: true, path: binPath, source: 'downloaded' };
  } finally {
    rmSync(lockPath, { force: true });
  }
}

/** Remove a server from the managed cache only (never touches external installs). */
export function uninstallServer(
  serverName: string,
  platformId: PlatformId = detectPlatformId()
): boolean {
  const binPath = cachedServerBinPath(serverName, platformId);
  if (!binPath) return false;
  const serverDir = path.dirname(path.dirname(binPath)); // <root>/<server>
  if (!existsSync(serverDir)) return false;
  // Guard: only ever delete inside the managed cache root.
  const root = managedCacheRoot();
  if (!path.resolve(serverDir).startsWith(path.resolve(root) + path.sep)) {
    return false;
  }
  rmSync(serverDir, { recursive: true, force: true });
  return true;
}
