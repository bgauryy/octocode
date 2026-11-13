// Repository paths the loopback providers and the recorder resolve against.
import path from 'node:path';

export const ROOT = path.resolve(import.meta.dirname, '../../../..');
/** The repository octo/demo serves: shared with the native structural tests' corpus. */
export const DEMO_REPO = path.join(ROOT, 'test-utils/fixtures/demo-repo');
/** The built CLI launcher. */
export const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
