import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { allowLocalExecutables } from '../../../../test-utils/external-effects-guard.js';
import { resolveNativeBin } from '../../src/cli/native-delegate.js';

export const launcher = resolve(__dirname, '..', '..', 'out', 'octocode.js');

/** The binary the launcher runs, resolved the way the launcher does. */
function nativeBinary(): string | null {
  const resolved = resolveNativeBin({});
  return 'bin' in resolved ? resolved.bin : null;
}

export const binary = nativeBinary();
if (binary) allowLocalExecutables(binary);
export const ready = existsSync(launcher) && binary !== null;
