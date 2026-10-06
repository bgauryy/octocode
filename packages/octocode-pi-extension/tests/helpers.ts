import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const created: string[] = [];

/** A fresh temporary directory, removed when the test process exits. */
export function tmp(prefix = 'octocode-pi-'): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  created.push(dir);
  return dir;
}

process.once('exit', () => {
  for (const dir of created) fs.rmSync(dir, { recursive: true, force: true });
});
