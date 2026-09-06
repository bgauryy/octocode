import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, vi } from 'vitest';

// Default-home flows are exercised against a private test home, never the
// developer's coordination database. Child processes inherit this same home.
const testHome = mkdtempSync(join(tmpdir(), 'awareness-test-home-'));
vi.stubEnv('OCTOCODE_HOME', testHome);
afterAll(() => { rmSync(testHome, { recursive: true, force: true }); });

process.removeAllListeners('warning');
process.on('warning', (warning) => {
  if (warning?.name === 'ExperimentalWarning' && String(warning?.message).includes('SQLite')) {
    return;
  }
  // Preserve non-SQLite warnings; only the noisy node:sqlite experimental notice is filtered.
  process.stderr.write(`${warning?.stack ?? String(warning)}\n`);
});
