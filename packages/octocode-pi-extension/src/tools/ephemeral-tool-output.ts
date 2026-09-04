import { randomBytes } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { extensionTmpRoot } from '../extension-paths.js';

const PRIVATE_DIR_MODE = 0o700;
const PRIVATE_FILE_MODE = 0o600;
const STALE_OUTPUT_MS = 24 * 60 * 60 * 1_000;
const createdFiles = new Set<string>();
let staleOutputsSwept = false;

function safePart(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64) || 'result';
}

/**
 * Persist tool output outside the conversation/session transcript. Files are
 * private, explicitly referenced from the bounded result, and removed when the
 * Pi session shuts down.
 */
export function writeEphemeralToolOutput(
  body: string | Uint8Array,
  options: { toolName: string; toolCallId?: string; extension?: string },
): string {
  const dir = path.join(extensionTmpRoot(), 'tool-results');
  fs.mkdirSync(dir, { recursive: true, mode: PRIVATE_DIR_MODE });
  try { fs.chmodSync(dir, PRIVATE_DIR_MODE); } catch { /* non-POSIX filesystem */ }
  if (!staleOutputsSwept) {
    staleOutputsSwept = true;
    const cutoff = Date.now() - STALE_OUTPUT_MS;
    try {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (!entry.isFile() || !/-p\d+-[a-f0-9]{16}\.[a-z0-9]+$/i.test(entry.name)) continue;
        const candidate = path.join(dir, entry.name);
        try {
          if (fs.statSync(candidate).mtimeMs < cutoff) fs.rmSync(candidate, { force: true });
        } catch {
          // Another session can remove its own file between listing and stat.
        }
      }
    } catch {
      // Stale cleanup is best effort and must not hide the current tool result.
    }
  }
  const id = randomBytes(8).toString('hex');
  const extension = safePart(options.extension ?? 'txt').replace(/^\.+/, '') || 'txt';
  const call = options.toolCallId ? `-${safePart(options.toolCallId)}` : '';
  const file = path.join(dir, `${safePart(options.toolName)}${call}-p${process.pid}-${id}.${extension}`);
  fs.writeFileSync(file, body, { mode: PRIVATE_FILE_MODE });
  try { fs.chmodSync(file, PRIVATE_FILE_MODE); } catch { /* non-POSIX filesystem */ }
  createdFiles.add(file);
  return file;
}

/** Remove only ephemeral output files created by this extension process. */
export function cleanupEphemeralToolOutputs(): number {
  let removed = 0;
  for (const file of createdFiles) {
    try {
      fs.rmSync(file, { force: true });
      removed += 1;
    } catch {
      // Best-effort shutdown cleanup; never turn teardown into a failure.
    }
  }
  createdFiles.clear();
  return removed;
}

export function chunkReadHint(file: string): string {
  return `Read only needed chunks with localGetFileContent(path=${JSON.stringify(file)}, charOffset=<offset>, charLength<=50000).`;
}
