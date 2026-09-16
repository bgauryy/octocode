import fs from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import {
  ensurePrivateDirectory,
  hardenPrivateFile,
  PRIVATE_FILE_MODE,
} from '@octocodeai/octocode-awareness/host';

function temporaryPath(filePath: string): string {
  return `${filePath}.${process.pid}.${randomUUID()}.tmp`;
}

function publish(filePath: string, content: string, mode?: number): void {
  const temporary = temporaryPath(filePath);
  try {
    fs.writeFileSync(temporary, content, {
      encoding: 'utf8',
      ...(mode === undefined ? {} : { mode }),
      flag: 'wx',
    });
    fs.renameSync(temporary, filePath);
  } finally {
    try { fs.rmSync(temporary, { force: true }); } catch { /* best-effort rollback */ }
  }
}

/** Publish durable state privately; readers never observe partial content. */
export function writePrivateFileAtomicSync(filePath: string, content: string): void {
  ensurePrivateDirectory(path.dirname(filePath));
  hardenPrivateFile(filePath);
  publish(filePath, content, PRIVATE_FILE_MODE);
  hardenPrivateFile(filePath);
}

/** Publish rebuildable workspace state atomically with inherited permissions. */
export function writeEphemeralFileAtomicSync(filePath: string, content: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  publish(filePath, content);
}
