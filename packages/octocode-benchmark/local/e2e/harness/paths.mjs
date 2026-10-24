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
/** The built CLI launcher. */
export const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
