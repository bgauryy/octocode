import { describe, it, expect, beforeEach } from 'vitest';
import path from 'node:path';
import {
  parseGitPorcelain,
  setPeerWipBaseline,
  isPeerWip,
  markOwnWrite,
  peerWipCount,
  peerWipNotice,
  resetPeerWipForTests,
} from '../src/tools/peer-wip.js';

beforeEach(() => resetPeerWipForTests());

describe('parseGitPorcelain', () => {
  it('parses modified/added/untracked/staged lines to relative paths', () => {
    const out = [
      ' M src/index.ts',
      'M  package.json',
      '?? new-file.ts',
      'A  added.ts',
    ].join('\n');
    expect(parseGitPorcelain(out)).toEqual(['src/index.ts', 'package.json', 'new-file.ts', 'added.ts']);
  });
  it('takes the NEW path for renames', () => {
    expect(parseGitPorcelain('R  old.ts -> new.ts')).toEqual(['new.ts']);
  });
  it('unquotes paths with spaces/special chars', () => {
    expect(parseGitPorcelain(' M "src/a b.ts"')).toEqual(['src/a b.ts']);
  });
  it('ignores blank lines', () => {
    expect(parseGitPorcelain('\n\n M a.ts\n')).toEqual(['a.ts']);
  });
});

describe('peer-wip baseline + ownership', () => {
  const cwd = '/repo';
  it('flags a pre-session dirty file as peer-WIP until this session writes it', () => {
    setPeerWipBaseline(cwd, ' M src/index.ts\nM  package.json\n');
    const idx = path.resolve(cwd, 'src/index.ts');
    expect(isPeerWip(idx)).toBe(true);
    expect(peerWipCount()).toBe(2);

    // A file NOT in the baseline is never peer-WIP.
    expect(isPeerWip(path.resolve(cwd, 'src/new.ts'))).toBe(false);

    // Once we write it, the warning stops and the count drops.
    markOwnWrite(idx);
    expect(isPeerWip(idx)).toBe(false);
    expect(peerWipCount()).toBe(1);
  });

  it('peerWipNotice returns an advisory only for peer-WIP paths', () => {
    setPeerWipBaseline(cwd, ' M src/index.ts\n');
    const idx = path.resolve(cwd, 'src/index.ts');
    expect(peerWipNotice(idx, 'src/index.ts')).toMatch(/already modified in the working tree/);
    expect(peerWipNotice(path.resolve(cwd, 'clean.ts'), 'clean.ts')).toBe('');
  });

  it('a fresh baseline clears prior ownership', () => {
    setPeerWipBaseline(cwd, ' M a.ts\n');
    markOwnWrite(path.resolve(cwd, 'a.ts'));
    setPeerWipBaseline(cwd, ' M a.ts\n');
    expect(isPeerWip(path.resolve(cwd, 'a.ts'))).toBe(true); // ownership reset with the new baseline
  });
});
