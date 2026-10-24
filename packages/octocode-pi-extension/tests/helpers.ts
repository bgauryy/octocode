import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fauxAssistantMessage, type fauxToolCall } from '@earendil-works/pi-ai';
import { PhotonImage } from '@silvia-odwyer/photon-node';
import type { Fetched } from '../src/web/fetch.js';

const created: string[] = [];

/** The extension entry the e2e suites load: the source, or the build (`dist/index.js`) with `OCTOCODE_E2E_ENTRY=dist`. */
export const E2E_ENTRY = path.join(import.meta.dirname, '..', process.env['OCTOCODE_E2E_ENTRY'] === 'dist' ? 'dist/index.js' : 'src/index.ts');

/** A scripted model reply that calls these tools. */
export const calls = (...toolCalls: ReturnType<typeof fauxToolCall>[]) => () => fauxAssistantMessage(toolCalls, { stopReason: 'toolUse' });
/** A scripted model reply that says `text`. */
export const say = (text: string) => () => fauxAssistantMessage(text);

/** A fresh temporary directory, removed when the test process exits. */
export function tmp(prefix = 'octocode-pi-'): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  created.push(dir);
  return dir;
}

process.once('exit', () => {
  for (const dir of created) fs.rmSync(dir, { recursive: true, force: true });
});

/** A fresh temporary directory marked as a repository root by an empty `.git`. */
export function repo(prefix?: string): string {
  const dir = tmp(prefix);
  fs.mkdirSync(path.join(dir, '.git'));
  return dir;
}

/** The pid of a process that has exited. */
export const deadPid = (): number => spawnSync(process.execPath, ['-e', '0']).pid;

/** Runs git in `cwd` and returns its trimmed output. */
export const git = (cwd: string, ...args: string[]): string => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();

/** Resolves once `check` holds, polling every 20 ms; rejects after `ms`. */
export async function until(check: () => boolean, ms = 5_000): Promise<void> {
  const end = Date.now() + ms;
  while (!check()) {
    if (Date.now() > end) throw new Error('condition not met in time');
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

/** A fetched page's text; a case that expects text fails on an image. */
export function asText(fetched: Fetched): string {
  if (typeof fetched !== 'string') throw new Error(`expected page text, got an ${fetched.mimeType} image`);
  return fetched;
}

/** A square RGBA png of incompressible pseudo-random pixels: `size` 512 encodes to about 1 MB. */
export function noisyPng(size: number): Buffer {
  const pixels = new Uint8Array(size * size * 4);
  let random = 17;
  for (let index = 0; index < pixels.length; index++) {
    random = (Math.imul(random, 1_664_525) + 1_013_904_223) >>> 0;
    pixels[index] = index % 4 === 3 ? 255 : random >>> 24;
  }
  const image = new PhotonImage(pixels, size, size);
  try {
    return Buffer.from(image.get_bytes());
  } finally {
    image.free();
  }
}

/** The pixel size of a decoded base64 or raw png/jpeg/webp image. */
export function pixelSize(image: string | Uint8Array): { width: number; height: number } {
  const decoded = PhotonImage.new_from_byteslice(typeof image === 'string' ? Buffer.from(image, 'base64') : image);
  try {
    return { width: decoded.get_width(), height: decoded.get_height() };
  } finally {
    decoded.free();
  }
}
