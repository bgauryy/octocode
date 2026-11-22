// The exclusive log creation is the claim shared by live and headless writers.
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

export function beginRun(out) {
  mkdirSync(out, { recursive: true });
  const occupied = () => new Error(`Graph output already exists in ${out}. Choose a new output directory or reopen the saved run with --replay.`);
  if (['data.json', 'stitched.json', 'server.json', 'chunks'].some(name => existsSync(join(out, name)))) throw occupied();
  try {
    writeFileSync(join(out, 'events.jsonl'), '', { flag: 'wx' });
  } catch (error) {
    if (error.code === 'EEXIST') throw occupied();
    throw error;
  }
}

// A fresh result directory under the workspace's .octocode/octocode-graph/.
export function freshOut(root) {
  const parent = resolve('.octocode/octocode-graph');
  mkdirSync(parent, { recursive: true });
  return mkdtempSync(join(parent, `${new Date().toISOString().slice(0, 10)}-${basename(root)}-`));
}
