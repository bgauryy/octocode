import { readFileSync, statSync } from 'node:fs';

const LIMIT = 4 * 1024 * 1024;
export function parseFlags(argv, valueFlags, booleanFlags = []) {
  const result = {};
  for (let index = 0; index < argv.length; index++) {
    const key = argv[index];
    if (booleanFlags.includes(key)) {
      if (Object.hasOwn(result, key)) throw new Error(`Duplicate option ${key}.`);
      result[key] = true;
      continue;
    }
    if (!valueFlags.includes(key) || Object.hasOwn(result, key) || !argv[index + 1] || argv[index + 1].startsWith('--')) throw new Error(`Invalid option ${key}.`);
    result[key] = argv[++index];
  }
  return result;
}
export function readJson(path) {
  let text;
  if (path === '-') {
    text = readFileSync(0, 'utf8');
    if (Buffer.byteLength(text) > LIMIT) throw new Error('stdin exceeds 4 MiB.');
  } else {
    if (statSync(path).size > LIMIT) throw new Error(`${path} exceeds 4 MiB.`);
    text = readFileSync(path, 'utf8');
  }
  return JSON.parse(text);
}
export function print(value, pretty = false) {
  process.stdout.write(JSON.stringify(value, null, pretty ? 2 : 0) + '\n');
}
export function stop(message, code = 2) {
  process.stderr.write(JSON.stringify({ error: { code, message } }) + '\n');
  process.exit(code);
}
