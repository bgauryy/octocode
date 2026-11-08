// Repository and suite paths the harness resolves against.
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const TESTING = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const ROOT = path.resolve(TESTING, '../../../..');
/** The benchmark corpora, relative to ROOT: the `{{T}}` / `$T` prefix of recipes and call mixes. */
export const REPOS_REL = 'packages/octocode-benchmark/local/repos';
export const REPOS = path.join(ROOT, REPOS_REL);
export const FIXTURES = path.join(TESTING, 'fixtures');
export const RESULTS = path.join(TESTING, 'results');
/** The built launchers' source folders: `build:dev` rewrites them in place. */
export const CLI_SOURCE_DIR = path.join(ROOT, 'packages/octocode/out');
export const MCP_SOURCE_DIR = path.join(ROOT, 'packages/octocode-mcp/dist');
/** Names the per-run build snapshot folder; set by the process that made it so child suites reuse it. */
export const SNAPSHOT_ENV = 'OCTOCODE_E2E_SNAPSHOT';
const snapshotDir = process.env[SNAPSHOT_ENV];
/** The built CLI launcher, and the MCP server entry: the run's snapshot copies once one exists (see build-snapshot.mjs). */
export let CLI = path.join(snapshotDir ? path.join(snapshotDir, 'cli') : CLI_SOURCE_DIR, 'octocode.js');
export let MCP_ENTRY = path.join(snapshotDir ? path.join(snapshotDir, 'mcp') : MCP_SOURCE_DIR, 'index.js');
/** Point every harness call at snapshot launchers. */
export function useBuildPaths({ cli, mcp }) { CLI = cli; MCP_ENTRY = mcp; }
