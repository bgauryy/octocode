import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, readdirSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { executableName, installExecutable, rustHostTarget } from './artifact-checks.mjs';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const hostTarget = rustHostTarget();
const target = process.env.CARGO_BUILD_TARGET ?? hostTarget;
const release = process.argv.includes('--release');
const flags = ['build', '--locked', '--manifest-path', join(root, 'Cargo.toml'), '--target', target];
if (release) flags.push('--release');
execFileSync('cargo', flags, { cwd: root, stdio: 'inherit' });
const filename = executableName(target);
const source = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), target, release ? 'release' : 'debug', filename);
const output = join(root, 'scripts');
const directory = output;
mkdirSync(directory, { recursive: true });
const destination = join(directory, filename);
// Reuse an unchanged executable; copying it needlessly forces fresh OS checks.
const installed = installExecutable(source, destination, { target, hostTarget });
const checksum = join(directory, 'SHA256SUMS');
const temporary = `${checksum}.${process.pid}.tmp`;
try {
  writeFileSync(temporary, `${installed.sha256}  ${filename}\n`);
  renameSync(temporary, checksum);
} finally { rmSync(temporary, { force: true }); }
// Refresh runtime output, keeping only the current executable and checksum.
for (const entry of readdirSync(output)) {
  if (![filename, 'SHA256SUMS'].includes(entry)) rmSync(join(output, entry), { recursive: true, force: true });
}
cpSync(join(root, 'src/runtime'), output, { recursive: true });
console.log(JSON.stringify({ executable: destination, ...installed }));
