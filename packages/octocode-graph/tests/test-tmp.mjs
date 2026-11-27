// Temporary directories for script tests, in the repository's scratch space: <repo>/.octocode/tmp/tests/octocode-graph/
// holds one folder per test process (named by its pid). The folder is removed when the process exits, pass or fail,
// and on SIGINT, SIGTERM or SIGHUP; one left by a killed process is removed by the next test process that starts.
import { mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../../../.octocode/tmp/tests/octocode-graph/', import.meta.url));

const alive = pid => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
};

let own;
function processDir() {
  if (own) return own;
  mkdirSync(ROOT, { recursive: true });
  for (const name of readdirSync(ROOT)) {
    const pid = Number(name.split('-')[0]);
    if (Number.isInteger(pid) && pid > 0 && pid !== process.pid && !alive(pid)) rmSync(join(ROOT, name), { recursive: true, force: true });
  }
  own = mkdtempSync(join(ROOT, `${process.pid}-`));
  const remove = () => rmSync(own, { recursive: true, force: true });
  process.on('exit', remove);
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.once(signal, () => {
      remove();
      process.kill(process.pid, signal);
    });
  }
  return own;
}

export function tempDir(prefix) {
  return mkdtempSync(join(processDir(), prefix));
}

// Source for the fake CLI scripts. They advertise themselves so tests exercise binary pinning as well.
export const fakeCatalog = `if (process.argv[2] === 'config') {
  console.log(JSON.stringify({ key: 'WORKSPACE_ROOT', value: process.env.WORKSPACE_ROOT ?? null }));
  process.exit(0);
}
const graphPackage = process.argv[2] === '__graph';
if (graphPackage) process.argv.splice(2, 1);
if (process.argv.includes('full')) {
  console.log(JSON.stringify({
    executable: process.argv[1],
    tools: graphPackage ? [{ name: 'graphQuery', maxQueries: 1 }] : process.env.TEST_NO_LSP ? [] : [{ name: 'lspSearch', maxQueries: 2 }],
    grammarCapabilities: [{ graphFacts: true, extensions: ['ts'] }],
  }));
  process.exit(0);
}`;
