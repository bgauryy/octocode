// Repository paths the recorder and its registry proxy resolve against.
import path from 'node:path';
import { ROOT } from '../lib/workspaces.mjs';

/** The built CLI launcher. */
export const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
