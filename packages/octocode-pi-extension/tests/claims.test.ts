import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { claimSuffix, releaseDeadClaims } from '../src/shared/claims.js';
import { processStartTime } from '../src/shared/process.js';
import { deadPid, tmp } from './helpers.js';

const MARKER = '.claim-';
const posix = process.platform !== 'win32';

/** Creates `names` in a fresh folder and returns it. */
function folder(...names: string[]): string {
  const dir = tmp();
  for (const name of names) fs.writeFileSync(path.join(dir, name), name);
  return dir;
}

const listed = (dir: string): string[] => fs.readdirSync(dir).toSorted();

describe('rename claims', () => {
  it("names the claimer by this process's pid and, where known, its start time", () => {
    const started = processStartTime(process.pid);
    expect(claimSuffix(MARKER)).toBe(`${MARKER}${process.pid}${started ? `-${started}` : ''}`);
  });

  it('gives back a claim whose claimer is gone, with or without a start time', async () => {
    const dir = folder(`a.json${MARKER}${deadPid()}`, `b.json${MARKER}${deadPid()}-1`);
    await releaseDeadClaims(dir, fs.readdirSync(dir), MARKER);
    expect(listed(dir)).toEqual(['a.json', 'b.json']);
  });

  it.runIf(posix)('gives back a claim whose pid now names another process, and keeps one its own process holds', async () => {
    // The claimer died and its pid went to this process's parent: the start time tells them apart.
    const held = `held.json${MARKER}${process.ppid}-${processStartTime(process.ppid)}`;
    const dir = folder(`reused.json${MARKER}${process.ppid}-1`, held);
    await releaseDeadClaims(dir, fs.readdirSync(dir), MARKER);
    expect(listed(dir)).toEqual([held, 'reused.json'].toSorted());
  });

  it('judges a claim with only a pid by the pid: a live one stays', async () => {
    const live = `live.json${MARKER}${process.pid}`;
    const dir = folder(live);
    await releaseDeadClaims(dir, fs.readdirSync(dir), MARKER);
    expect(listed(dir)).toEqual([live]);
  });

  it('leaves names that are no claim by this marker, and claims on originals the caller does not own', async () => {
    const other = `x.json.pruning-${deadPid()}`;
    const foreign = `x.log${MARKER}${deadPid()}`;
    const dir = folder('plain.json', other, foreign, `${MARKER}${deadPid()}`, `y.json${MARKER}abc`);
    const before = listed(dir);
    await releaseDeadClaims(dir, fs.readdirSync(dir), MARKER, (original) => original.endsWith('.json'));
    expect(listed(dir)).toEqual(before);
  });

  it('skips a claim another process gave back first', async () => {
    const dir = folder();
    await expect(releaseDeadClaims(dir, [`gone.json${MARKER}${deadPid()}`], MARKER)).resolves.toBeUndefined();
    expect(listed(dir)).toEqual([]);
  });
});
