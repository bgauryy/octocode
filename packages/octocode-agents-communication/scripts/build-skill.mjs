import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { installExecutable } from './artifact-checks.mjs';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const hostTarget = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const target = process.env.CARGO_BUILD_TARGET ?? hostTarget;
const release = process.argv.includes('--release');
const flags = ['build', '--locked', '--manifest-path', join(root, 'Cargo.toml'), '--target', target];
if (release) flags.push('--release');
execFileSync('cargo', flags, { cwd: root, stdio: 'inherit' });
const filename = `octocode-agents-communication${target.includes('windows') ? '.exe' : ''}`;
const source = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), target, release ? 'release' : 'debug', filename);
const directory = join(root, 'skills/octocode-agents-communication/scripts/bin', target);
mkdirSync(directory, { recursive: true });
const destination = join(directory, filename);
const installed = installExecutable(source, destination, { target, hostTarget });
const checksum = join(directory, 'SHA256SUMS');
const temporary = `${checksum}.${process.pid}.tmp`;
try {
  writeFileSync(temporary, `${installed.sha256}  ${filename}\n`);
  renameSync(temporary, checksum);
} finally { rmSync(temporary, { force: true }); }
console.log(JSON.stringify({ executable: destination, ...installed }));
