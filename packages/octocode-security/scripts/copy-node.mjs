/**
 * copy-node.mjs — copies the compiled .dylib / .so to the correct .node filename
 */
import { copyFileSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..');
const dev = process.argv.includes('--dev');
const profile = dev ? 'debug' : 'release';

const platform = process.platform;
const arch = process.arch;

const tripleMap = {
  darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
  linux:  { arm64: 'linux-arm64-gnu', x64: 'linux-x64-gnu' },
  win32:  { x64: 'win32-x64-msvc' },
};
const triple = tripleMap[platform]?.[arch];
if (!triple) {
  console.error(`Unsupported platform: ${platform}-${arch}`);
  process.exit(1);
}

const extMap = { darwin: '.dylib', linux: '.so', win32: '.dll' };
const srcExt = extMap[platform];
const src = join(root, 'target', profile, `liboctocode_security${srcExt}`);
const dst = join(root, `octocode-security.${triple}.node`);

if (!existsSync(src)) {
  console.error(`Build artifact not found: ${src}`);
  process.exit(1);
}

copyFileSync(src, dst);
console.log(`✅ Copied ${profile} binary → ${dst}`);
