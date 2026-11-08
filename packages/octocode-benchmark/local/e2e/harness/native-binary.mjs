// The native binary every surface (CLI, launchers, MCP) runs, for run provenance.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import path from 'node:path';
import { devOverridesAllowed } from '@octocodeai/config';
import { ROOT, SNAPSHOT_ENV } from './paths.mjs';

/** Resolve once for both provenance and the processes that execute the tools. */
export function nativeBinaryPath() {
  const { resolveNativeBinaryPath } = createRequire(import.meta.url)(path.join(ROOT, 'packages/octocode-native/bin/resolve-binary.cjs'));
  const requested = process.env.OCTOCODE_NATIVE_BIN?.trim();
  return path.resolve(requested && devOverridesAllowed(process.env, { bundled: true }) ? requested : resolveNativeBinaryPath());
}

/** The run's build snapshot manifest (build-snapshot.mjs), or null when the run reads the build live. */
function snapshotManifest() {
  const dir = process.env[SNAPSHOT_ENV];
  if (!dir) return null;
  try { return JSON.parse(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf8')); } catch { return null; }
}

/** Source files whose current mtime or size differs from the snapshot's: the build moved during the run. */
function changedSince(group) {
  const source = path.join(ROOT, group.source);
  return group.files.filter(entry => {
    try {
      const file = fs.statSync(source).isDirectory() ? path.join(source, entry.name) : source;
      const stat = fs.statSync(file);
      return stat.mtime.toISOString() !== entry.sourceMtime || stat.size !== entry.bytes;
    } catch { return true; }
  }).map(entry => entry.name);
}

/**
 * The resolved native artifact, including content identity, or null when none is installed.
 * Under a build snapshot the identity is the copy's current content (so `stable` proves the copy
 * stayed intact) and `snapshot` records the copies' sha256 and the sources' mtimes.
 */
export function nativeBinaryStamp() {
  try {
    const file = nativeBinaryPath();
    const stat = fs.statSync(file);
    const sha256 = createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    const manifest = snapshotManifest();
    if (manifest?.native.copy !== file) return { file: path.relative(ROOT, file), bytes: stat.size, mtime: stat.mtime.toISOString(), sha256 };
    const { native, cli, mcp } = manifest;
    return {
      file: native.source, bytes: stat.size, mtime: native.files[0].sourceMtime, sha256,
      snapshot: {
        createdAt: manifest.createdAt,
        native: { source: native.source, sourceMtime: native.files[0].sourceMtime, sha256: native.files[0].sha256, changedSinceSnapshot: changedSince(native).length > 0 },
        cli: { source: cli.source, files: cli.files, changedSinceSnapshot: changedSince(cli) },
        mcp: { source: mcp.source, files: mcp.files, changedSinceSnapshot: changedSince(mcp) },
      },
    };
  } catch { return null; }
}
