import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { python } from './artifact-checks.mjs';

execFileSync(python(), ['-B', '-m', 'unittest', 'discover', '-s', 'tests', '-p', '*_test.py'], {
  cwd: fileURLToPath(new URL('../', import.meta.url)), stdio: 'inherit',
});
