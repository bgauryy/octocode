import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, copyFileSync, chmodSync, statSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
if (process.argv.includes('--help')) {
  console.log('Build the host Rust binary: npm run build. Requires Rust >=1.85 and a C linker. Uses Cargo.lock; no cross-compilation.');
  process.exit(0);
}
if (process.argv.length > 2) throw new Error('Unexpected build argument; use --help.');
const temporary = !process.env.CARGO_TARGET_DIR;
const target = process.env.CARGO_TARGET_DIR || mkdtempSync(join(tmpdir(), 'octocode-jev-build-'));
try {
  const result = spawnSync('cargo', ['build', '--release', '--locked', '--manifest-path', join(root, 'Cargo.toml'), '--target-dir', target], { stdio: 'inherit' });
  if (result.error || result.status !== 0) {
    console.error('Build failed. Install Rust >=1.85 and a C linker, then run npm run build.');
    process.exitCode = result.status || 1;
  } else {
    const suffix = process.platform === 'win32' ? '.exe' : '';
    const dest = join(root, 'bin', `octocode-jev-${process.platform}-${process.arch}${suffix}`);
    mkdirSync(join(root, 'bin'), { recursive: true });
    copyFileSync(join(target, 'release', `octocode-jev${suffix}`), dest);
    chmodSync(dest, 0o755);
    console.log(JSON.stringify({ binary: dest, bytes: statSync(dest).size, platform: process.platform, arch: process.arch }));
  }
} finally { if (temporary) rmSync(target, { recursive: true, force: true }); }
