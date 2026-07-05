import { createHash } from 'node:crypto';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';

import { detectPlatformId, type PlatformId } from './platform.js';
import { MANIFEST } from './serverManifestData.js';

/**
 * Download manifest for portable, toolchain-free language servers (the
 * AUTO-DOWNLOAD-OK class). `serverManifest.json` is the human-authored source;
 * `serverManifestData.ts` mirrors it as a TS module so the compiled dist needs
 * no JSON-copy step. This module is the typed loader + per-platform selector +
 * managed-cache locator.
 *
 * Provisioning policy (see docs/context/LSP_GUIDE.md):
 *   OCTOCODE_LSP_AUTO_INSTALL = prompt (default) | off | auto
 * Live network download is gated and requires a pinned `sha256` per asset.
 * Until SHAs are pinned the manifest still drives (a) honest detect-and-instruct
 * guidance and (b) reuse of a server a user/CI has pre-populated into the
 * managed cache `~/.octocode/lsp/<server>/<releaseTag>/<binName>`.
 */
export type ArchiveKind = 'none' | 'gz' | 'zip' | 'tar.gz' | 'tar.xz';

export interface ManifestAsset {
  url: string;
  archive: ArchiveKind;
  binName: string;
  /** Path of the executable inside the archive (zip/tar); absent for gz/none. */
  binPath?: string;
  /** SHA-256 of the downloaded asset; download is refused while this is null. */
  sha256: string | null;
}

export interface ManifestServer {
  languageId: string;
  repo: string;
  releaseTag: string;
  launchArgs?: string[];
  downloadHost?: string;
  platforms: Partial<Record<PlatformId, ManifestAsset>>;
  unsupportedPlatforms?: Partial<Record<PlatformId, string>>;
}

export interface ManifestFile {
  $comment?: string;
  version: number;
  servers: Record<string, ManifestServer>;
}

export type ProvisionMode = 'off' | 'prompt' | 'auto';

interface CacheMarker {
  binarySha256: string;
  size: number;
}

function loadManifest(): ManifestFile {
  return MANIFEST;
}

function sha256File(filePath: string): string | null {
  try {
    return createHash('sha256').update(readFileSync(filePath)).digest('hex');
  } catch {
    return null;
  }
}

function readCacheMarker(markerPath: string): CacheMarker | null {
  try {
    const parsed = JSON.parse(readFileSync(markerPath, 'utf8')) as Partial<CacheMarker>;
    if (
      typeof parsed.binarySha256 !== 'string' ||
      !/^[0-9a-f]{64}$/u.test(parsed.binarySha256) ||
      typeof parsed.size !== 'number' ||
      !Number.isSafeInteger(parsed.size) ||
      parsed.size < 0
    ) {
      return null;
    }
    return { binarySha256: parsed.binarySha256, size: parsed.size };
  } catch {
    return null;
  }
}

/** The configured auto-install policy. Defaults to `prompt` when unset (asks before downloading). Set OCTOCODE_LSP_AUTO_INSTALL=off to disable all downloads, or =auto to skip the prompt. */
export function provisionMode(
  env: NodeJS.ProcessEnv = process.env
): ProvisionMode {
  const raw = (env.OCTOCODE_LSP_AUTO_INSTALL ?? '').trim().toLowerCase();
  if (raw === 'auto' || raw === 'off') return raw;
  return 'prompt';
}

/** The manifest entry for a server, keyed by its bare command name. */
export function manifestServer(serverName: string): ManifestServer | null {
  return loadManifest().servers[path.basename(serverName)] ?? null;
}

/** Every auto-downloadable server in the manifest (for `lsp-server list`). */
export function listManifestServers(): Array<{
  name: string;
  languageId: string;
  releaseTag: string;
}> {
  return Object.entries(loadManifest().servers).map(([name, server]) => ({
    name,
    languageId: server.languageId,
    releaseTag: server.releaseTag,
  }));
}

/** Whether the manifest can, in principle, auto-provide this server. */
export function isAutoDownloadable(serverName: string): boolean {
  return manifestServer(serverName) != null;
}

/**
 * Root of the managed server cache. Defaults to `~/.octocode/lsp` (consistent
 * with the rest of octocode's home), overridable via `OCTOCODE_LSP_CACHE_DIR`
 * for read-only/ephemeral sandbox HOMEs or to point at a pre-baked image path.
 */
export function managedCacheRoot(
  env: NodeJS.ProcessEnv = process.env
): string {
  const override = env.OCTOCODE_LSP_CACHE_DIR?.trim();
  if (override) return path.resolve(override);
  return path.join(homedir(), '.octocode', 'lsp');
}

/** Where a provisioned binary lives once installed/extracted. */
export function cachedServerBinPath(
  serverName: string,
  platformId: PlatformId = detectPlatformId()
): string | null {
  const server = manifestServer(serverName);
  const asset = server?.platforms[platformId];
  if (!server || !asset) return null;
  return path.join(
    managedCacheRoot(),
    path.basename(serverName),
    server.releaseTag,
    asset.binName
  );
}

/**
 * If a managed binary is already present AND verified in the cache (downloaded
 * by a prior run, by CI, or pre-baked), return its absolute path. A binary is
 * only trusted when its sibling `<binName>.ok` completion marker contains the
 * current binary's SHA-256 and size. A half-written or tampered binary from an
 * interrupted install has no matching marker and is ignored, so it never
 * resolves as "installed". Read-only — always safe to call.
 */
export function resolveCachedServer(
  serverName: string,
  platformId: PlatformId = detectPlatformId()
): string | null {
  const binPath = cachedServerBinPath(serverName, platformId);
  if (!binPath || !existsSync(binPath)) return null;

  const marker = readCacheMarker(`${binPath}.ok`);
  if (!marker) return null;

  let size: number;
  try {
    size = statSync(binPath).size;
  } catch {
    return null;
  }
  if (size !== marker.size) return null;

  return sha256File(binPath) === marker.binarySha256 ? binPath : null;
}

/** Human-readable provisioning guidance for a server with no resolved binary. */
export function manifestInstallHint(serverName: string): string | null {
  const server = manifestServer(serverName);
  if (!server) return null;
  const platformId = detectPlatformId();
  const unsupported = server.unsupportedPlatforms?.[platformId];
  if (unsupported) return unsupported;
  const mode = provisionMode();
  if (mode === 'off') {
    return `${serverName} can be auto-downloaded from ${server.repo} (${server.releaseTag}). Re-enable with OCTOCODE_LSP_AUTO_INSTALL=prompt (or =auto), or run \`lsp-server install ${serverName}\`.`;
  }
  return `${serverName} (${server.releaseTag}) will be auto-downloaded on first use from ${server.repo}.`;
}
