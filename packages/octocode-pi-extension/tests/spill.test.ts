import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { setCurrentSession } from '../src/shared/home.js';
import { capOutputToFile, saveFullOutput, spillLabel } from '../src/shared/spill.js';
import { tmp } from './helpers.js';

let home: string;
let saved: string | undefined;

beforeEach(() => {
  saved = process.env['OCTOCODE_HOME'];
  home = tmp();
  process.env['OCTOCODE_HOME'] = home;
});

afterEach(() => {
  setCurrentSession(undefined);
  if (saved === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = saved;
});

const manyLines = (count: number) => Array.from({ length: count }, (_, index) => `line ${index + 1}`).join('\n');
const savedPath = (text: string) => /Full output: (\S+?); search it/.exec(text)?.[1];

describe('capOutputToFile', () => {
  it('returns text within the limits unchanged and writes nothing', () => {
    const text = manyLines(100);
    expect(capOutputToFile(text, { label: 'x' })).toBe(text);
    expect(fs.existsSync(path.join(home, 'agent', 'pi', 'sessions'))).toBe(false);
  });

  it('saves over-limit text whole and previews its head and tail', () => {
    const text = manyLines(5_000);
    const out = capOutputToFile(text, { label: 'octocode/local search' });
    const file = savedPath(out)!;
    // No session yet (a unit test): a pid-named folder the sweep drops once the process is gone.
    expect(path.dirname(file)).toBe(path.join(home, 'agent', 'pi', 'sessions', `_pid-${process.pid}`, 'output'));
    expect(path.basename(file)).toMatch(/^octocode_local_search-\d+-\d+\.txt$/);
    expect(fs.readFileSync(file, 'utf8')).toBe(text);
    expect(out.startsWith('line 1\n')).toBe(true);
    expect(out).toContain('\nline 5000\n');
    expect(out).toMatch(/\[… \d+ lines omitted …\]/);
    expect(out).toMatch(/\[Output truncated: showing \d+ of 5000 lines \(.+ of .+\)\. Full output: .+; search it or read it by line range\.\]$/);
    expect(out.split('\n').length).toBeLessThan(2_000);
  });

  it('saves into the current session folder once a session is set', () => {
    setCurrentSession('abc/1');
    const file = savedPath(capOutputToFile(manyLines(5_000), { label: 'x' }))!;
    expect(path.dirname(file)).toBe(path.join(home, 'agent', 'pi', 'sessions', 'abc_1', 'output'));
  });

  it('previews a single over-long line by bytes at both ends', () => {
    const text = `START${'y'.repeat(120_000)}END`;
    const out = capOutputToFile(text, { label: 'one' });
    expect(out.startsWith('START')).toBe(true);
    expect(out).toContain('END\n\n[Output truncated');
    expect(Buffer.byteLength(out)).toBeLessThan(52_000);
    expect(fs.readFileSync(savedPath(out)!, 'utf8')).toBe(text);
  });

  it('falls back to a head cap and says so when the folder cannot be written', () => {
    const blocker = path.join(tmp(), 'not-a-dir');
    fs.writeFileSync(blocker, '');
    process.env['OCTOCODE_HOME'] = blocker;
    const out = capOutputToFile(manyLines(5_000), { label: 'x' });
    expect(out).toContain('[Output truncated: ');
    expect(out.endsWith('[full output could not be saved]')).toBe(true);
    expect(saveFullOutput('x', 'x')).toBeUndefined();
  });

  it('keeps labels to safe file-name characters', () => {
    expect(spillLabel('a/b c..d')).toBe('a_b_c__d');
    expect(spillLabel('')).toBe('output');
  });
});

