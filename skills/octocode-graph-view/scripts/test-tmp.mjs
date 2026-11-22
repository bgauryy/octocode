// Temporary directories for script tests: each one is removed when the test process exits, pass or fail.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const made = new Set();
process.on('exit', () => {
  for (const dir of made) rmSync(dir, { recursive: true, force: true });
});

export function tempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  made.add(dir);
  return dir;
}

// Source for the fake CLI scripts. They advertise themselves so tests exercise binary pinning as well.
export const fakeCatalog = `if (process.argv.includes('full')) {
  console.log(JSON.stringify({
    executable: process.argv[1],
    tools: [{ name: 'graphQuery', maxQueries: 1 }, { name: 'lspSearch', maxQueries: 2 }],
    grammarCapabilities: [{ graphFacts: true, extensions: ['ts'] }],
  }));
  process.exit(0);
}`;
