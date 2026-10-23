// The native binary every surface (CLI, launchers, MCP) runs, for run provenance.
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { ROOT } from './mcp-client.mjs';

/** The native binary every surface runs: its file, size and mtime, or null when none is installed. */
export function nativeBinaryStamp() {
  try {
    const { resolveNativeBinaryPath } = createRequire(import.meta.url)(path.join(ROOT, 'packages/octocode-native/bin/resolve-binary.cjs'));
    const file = process.env.OCTOCODE_NATIVE_BIN ?? resolveNativeBinaryPath();
    const stat = fs.statSync(file);
    return { file: path.relative(ROOT, file), bytes: stat.size, mtime: stat.mtime.toISOString() };
  } catch { return null; }
}
