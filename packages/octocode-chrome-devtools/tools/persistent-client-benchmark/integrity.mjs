// Runtime and harness fingerprints: a benchmark is valid only if the measured build and
// harness stay byte-identical from start to finish.
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';

export const hash = value => createHash('sha256').update(value).digest('hex');
const distDirectory = new URL('../../dist/', import.meta.url);
export const runtimeFile = new URL('../../dist/runtime.js', import.meta.url);

/** Digest of every file under dist/ keyed by its relative path. */
export const distFingerprint = () =>
  hash(
    readdirSync(distDirectory, { recursive: true, withFileTypes: true })
      .filter(entry => entry.isFile())
      .map(entry => {
        const file = resolve(entry.parentPath, entry.name);
        return [
          file.slice(new URL(distDirectory).pathname.length),
          hash(readFileSync(file)),
        ];
      })
      .toSorted((a, b) => a[0].localeCompare(b[0]))
      .map(row => JSON.stringify(row))
      .join('\n')
  );

/** Digest of the entry script and every harness module, in a fixed order. */
export const harnessFingerprint = entry =>
  hash(
    [
      entry,
      ...['integrity', 'session', 'workload', 'guardrails'].map(
        name => new URL(`./${name}.mjs`, import.meta.url)
      ),
    ]
      .map(file => readFileSync(file))
      .join('\n')
  );
