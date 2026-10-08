import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

// Tests must never touch the user's real ~/.octocode (its agent database is shared with every running Pi): tests that
// "restore" OCTOCODE_HOME/OCTOCODE_AGENT_DB restore these per-worker temporary paths, so late async work stays sandboxed.
const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-test-home-'));
process.env['OCTOCODE_HOME'] = home;
delete process.env['OCTOCODE_AGENT_DB'];
