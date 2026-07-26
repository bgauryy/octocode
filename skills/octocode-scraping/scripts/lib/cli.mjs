import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

export function takeArg(args, flag) {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : '';
}

export async function readJson(dir, rel, fallback = null) {
  return existsSync(join(dir, rel)) ? JSON.parse(await readFile(join(dir, rel), 'utf8')) : fallback;
}

export async function readJsonl(dir, rel) {
  if (!existsSync(join(dir, rel))) return [];
  return (await readFile(join(dir, rel), 'utf8')).trim().split('\n').filter(Boolean).map((line) => JSON.parse(line));
}
