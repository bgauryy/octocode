import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterAll } from 'vitest';

// Each isolated test file gets a fresh home before its modules are imported.
// Reusing a machine-wide temp home leaks ledgers and schema versions across runs.
const previousHome = process.env.OCTOCODE_HOME;
const inheritedBindingKeys = [
  'OCTOCODE_AWARENESS_DB',
  'OCTOCODE_AWARENESS_DB_PATH',
  'OCTOCODE_AWARENESS_WORKSPACE',
  'OCTOCODE_AGENT_ID',
] as const;
const previousBindings = new Map(inheritedBindingKeys.map(key => [key, process.env[key]]));
const testRoot = mkdtempSync(path.join(tmpdir(), 'octocode-pi-storage-'));
const testHome = path.join(testRoot, '.octocode');
process.env.OCTOCODE_HOME = testHome;
for (const key of inheritedBindingKeys) delete process.env[key];

afterAll(() => {
  if (previousHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = previousHome;
  for (const key of inheritedBindingKeys) {
    const previous = previousBindings.get(key);
    if (previous === undefined) delete process.env[key];
    else process.env[key] = previous;
  }
  rmSync(testRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
});
