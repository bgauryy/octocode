// The native binary every surface (CLI, launchers, MCP) runs, for run provenance.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import path from 'node:path';
import { ROOT } from './paths.mjs';

/** Resolve once for both provenance and the processes that execute the tools. */
export function nativeBinaryPath() {
  const { resolveNativeBinaryPath } = createRequire(import.meta.url)(path.join(ROOT, 'packages/octocode-native/bin/resolve-binary.cjs'));
  return path.resolve(process.env.OCTOCODE_NATIVE_BIN ?? resolveNativeBinaryPath());
}

/** The resolved native artifact, including content identity, or null when none is installed. */
export function nativeBinaryStamp() {
  try {
    const file = nativeBinaryPath();
    const stat = fs.statSync(file);
    return { file: path.relative(ROOT, file), bytes: stat.size, mtime: stat.mtime.toISOString(), sha256: createHash('sha256').update(fs.readFileSync(file)).digest('hex') };
  } catch { return null; }
}
