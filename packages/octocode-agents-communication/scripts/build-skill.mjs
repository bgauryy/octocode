import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, readFileSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { createHash } from 'node:crypto';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const target = process.env.CARGO_BUILD_TARGET ?? execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const release = process.argv.includes('--release');
const flags = ['build', '--locked', '--manifest-path', join(root, 'Cargo.toml'), '--target', target];
if (release) flags.push('--release');
execFileSync('cargo', flags, { cwd: root, stdio: 'inherit' });
const filename = `octocode-agents-communication${target.includes('windows') ? '.exe' : ''}`;
const source = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), target, release ? 'release' : 'debug', filename);
const directory = join(root, 'skills/octocode-agents-communication/scripts/bin', target);
mkdirSync(directory, { recursive: true });
const destination = join(directory, filename);
const temporary = `${destination}.${process.pid}.tmp`;
try {
  copyFileSync(source, temporary);
  chmodSync(temporary, 0o755);
  renameSync(temporary, destination);
} finally { rmSync(temporary, { force: true }); }
writeFileSync(join(directory, 'SHA256SUMS'), `${createHash('sha256').update(readFileSync(destination)).digest('hex')}  ${filename}\n`);
console.log(`Skill executable: ${destination}`);
