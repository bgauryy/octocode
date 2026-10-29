// Shared helpers: flag parsing, async octocode CLI calls, a small worker pool.
import { execFile, execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { cropScale } from './png.mjs';
import { join, resolve } from 'node:path';

// Directories a graph ingest never reads; a size measure skips the same ones.
export const SKIPPED_DIRS = ['target', 'node_modules', '.git', 'dist', 'build', '.octocode'];

// Bytes and count of the files under `root` that a graph ingest could read: hidden entries,
// SKIPPED_DIRS, `exclude` and files over 1 MB (binaries, archives; the same bound as the frozen
// copy) are left out, so build output and staged binaries never inflate the source measure.
export const MAX_SOURCE_FILE = 1 << 20;
export function sourceSize(root, exclude = []) {
  const skip = new Set([...SKIPPED_DIRS, ...exclude]);
  let bytes = 0;
  let files = 0;
  (function walk(dir) {
    for (const d of readdirSync(dir, { withFileTypes: true })) {
      if (d.name.startsWith('.')) continue;
      const p = join(dir, d.name);
      if (d.isDirectory()) {
        if (!skip.has(d.name)) walk(p);
      } else if (d.isFile()) {
        const size = statSync(p).size;
        if (size > MAX_SOURCE_FILE) continue;
        bytes += size;
        files++;
      }
    }
  })(root);
  return { bytes, files };
}

export function flags(argv, spec) {
  const out = {};
  for (const [name, dflt] of Object.entries(spec)) {
    const i = argv.indexOf(`--${name}`);
    if (i >= 0 && (argv[i + 1] === undefined || argv[i + 1].startsWith('--'))) {
      console.error(`--${name} needs a value`);
      process.exit(2);
    }
    out[name] = i < 0 ? dflt : argv.splice(i, 2)[1];
  }
  return out;
}

// Repo rule: use the local launcher inside the checkout; OCTOCODE_CLI overrides; otherwise `octocode` on PATH.
const local = resolve('packages/octocode/out/octocode.js');
const cli = process.env.OCTOCODE_CLI ? process.env.OCTOCODE_CLI.split(' ') : existsSync(local) ? [process.execPath, local] : ['octocode'];

// `octocode` exits 1 on an empty result but still prints one JSON object; other failures reject.
export function octocode(args, { timeout = 0 } = {}) {
  return new Promise((resolve, reject) => {
    execFile(cli[0], [...cli.slice(1), ...args], { maxBuffer: 256 << 20, timeout }, (err, stdout, stderr) => {
      if (!err || err.code === 1) {
        try {
          return resolve(JSON.parse(stdout));
        } catch {}
      }
      reject(new Error(`octocode ${args.slice(0, 2).join(' ')}: ${String(stderr || stdout || err?.message || 'no output').slice(0, 500)}`));
    });
  });
}

export async function pool(items, size, fn) {
  const out = new Array(items.length);
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(size, items.length) }, async () => {
      while (next < items.length) {
        const i = next++;
        out[i] = await fn(items[i], i);
      }
    }),
  );
  return out;
}

export function chrome() {
  const found = [
    process.env.CHROME_PATH,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ].find((p) => p && existsSync(p));
  if (!found) throw new Error('Chrome not found: set CHROME_PATH');
  return found;
}

// Crop a pixel box (optionally scaled up for OCR); sips on macOS, ImageMagick elsewhere.
export function crop(src, box, dst, scale = 1) {
  try {
    writeFileSync(dst, cropScale(readFileSync(src), box, scale));
    return;
  } catch {} // not a PNG this module decodes (16-bit, interlaced): fall back to sips or ImageMagick below
  const x = Math.max(0, Math.round(box.x)), y = Math.max(0, Math.round(box.y));
  const w = Math.round(box.w), h = Math.round(box.h);
  try {
    execFileSync('sips', ['-c', String(h), String(w), '--cropOffset', String(y), String(x), src, '--out', dst], { stdio: 'ignore' });
    if (scale !== 1) execFileSync('sips', ['-z', String(h * scale), String(w * scale), dst], { stdio: 'ignore' });
  } catch {
    execFileSync('magick', [src, '-crop', `${w}x${h}+${x}+${y}`, '+repage', ...(scale !== 1 ? ['-resize', `${scale * 100}%`] : []), dst], { stdio: 'ignore' });
  }
}

// True when `octocode <args>` exits 0 (help text is not JSON, so octocode() cannot be used for it).
export function cliOk(args) {
  try {
    execFileSync(cli[0], [...cli.slice(1), ...args], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}
