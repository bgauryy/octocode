import { mkdtempSync, readFileSync, rmSync, statSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it } from 'vitest';
import { writeGenerated } from '../scripts/generated-files.ts';

it('leaves unchanged generated source untouched and publishes changed or missing source', () => {
  const dir = mkdtempSync(join(tmpdir(), 'generated-write-'));
  try {
    const path = join(dir, 'types.rs');
    writeGenerated(path, 'first');
    // A fixed old mtime detects a rewrite without a sleep or filesystem clock race.
    const old = new Date('2000-01-01T00:00:00Z');
    utimesSync(path, old, old);
    const previous = statSync(path).mtimeMs;
    writeGenerated(path, 'first');
    expect(statSync(path).mtimeMs).toBe(previous);
    writeGenerated(path, 'second');
    expect(readFileSync(path, 'utf8')).toBe('second');
    expect(statSync(path).mtimeMs).not.toBe(previous);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
