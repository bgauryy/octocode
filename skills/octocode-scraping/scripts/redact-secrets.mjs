#!/usr/bin/env node
import { readFile, rename, rm, writeFile } from 'node:fs/promises';
import { existsSync, statSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import { redactSecretsInText } from './lib/bridge.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const HELP = `Usage: redact-secrets.mjs --file <path> [--file <path>]... --in-place
       redact-secrets.mjs --file <path> --out <path>
Masks credentials in saved text (reports, snippets, extracts) with the same rules har-ingest applies to captures:
known token shapes (JWT, GitHub, AWS, Slack, OpenAI-style, Google API keys, private keys), Bearer/Basic/Token values,
and the values of credential-named JSON fields, form/query pairs and header lines. Each value becomes [REDACTED];
surrounding text is unchanged. Personal data (names, emails, phone numbers) is not matched.
--in-place rewrites each file atomically; --out writes the one --file's redacted text to <path>.
Prints JSON {ok, files: [{file, out, redactions, changed}]}; never prints file text.
Exit 2 on invalid input (invalidArguments: no --file, a missing file, neither or both of --in-place and --out,
--out with several files); exit 1 when a read or write fails (writeFailed).`;

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) { console.log(HELP); process.exit(0); }

const files = [], valueFlags = new Set(['--file', '--out']);
let out = null;
for (let i = 0; i < args.length; i += 1) {
  const flag = args[i];
  if (flag === '--in-place') continue;
  if (!valueFlags.has(flag)) fail('invalidArguments', `unknown argument ${flag}`, 2);
  const value = args[i + 1];
  if (!value || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
  if (flag === '--file') files.push(resolve(value)); else out = resolve(value);
  i += 1;
}
const inPlace = args.includes('--in-place');
if (!files.length) fail('invalidArguments', '--file is required', 2);
if (inPlace === Boolean(out)) fail('invalidArguments', 'pass exactly one of --in-place or --out', 2);
if (out && files.length > 1) fail('invalidArguments', '--out takes one --file; use --in-place for several', 2);
for (const file of files) if (!existsSync(file) || !statSync(file).isFile()) fail('invalidArguments', `not a file: ${file}`, 2);

const marks = (text) => text.split('[REDACTED]').length - 1;
async function writeAtomic(path, text) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  try { await writeFile(temporary, text); await rename(temporary, path); } finally { await rm(temporary, { force: true }); }
}

const results = [];
try {
  for (const file of files) {
    const text = await readFile(file, 'utf8');
    const redacted = redactSecretsInText(text);
    const target = out || file;
    if (out || redacted !== text) await writeAtomic(target, redacted);
    results.push({ file, out: target, redactions: marks(redacted) - marks(text), changed: redacted !== text });
  }
} catch (error) { fail('writeFailed', errorMessage(error), 1, { files: results }); }
console.log(JSON.stringify({ ok: true, files: results }, null, 2));
