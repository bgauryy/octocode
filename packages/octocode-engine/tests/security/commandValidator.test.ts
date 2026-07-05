import { describe, expect, it } from 'vitest';

import { validateCommand } from '../../src/security/commandValidator.js';

const SAFE_ARCHIVE_BACKEND_COMMANDS: Array<[string, string[]]> = [
  ['file', ['--mime-type', '-b', '/tmp/archive.tar.gz']],
  ['zcat', ['/tmp/archive.gz']],
  ['gunzip', ['-c', '/tmp/archive.gz']],
  ['bzcat', ['/tmp/archive.bz2']],
  ['xzcat', ['/tmp/archive.xz']],
  ['xzcat', ['--format=lzma', '/tmp/archive.lzma']],
  ['zstdcat', ['/tmp/archive.zst']],
  ['zstd', ['-dcq', '/tmp/archive.zst']],
  ['lz4cat', ['/tmp/archive.lz4']],
  ['brotli', ['-dc', '/tmp/archive.br']],
  ['lzfse', ['-decode', '-i', '/tmp/archive.lzfse', '-o', '/dev/stdout']],
  ['tar', ['-xOf', '/tmp/archive.tar', '--', 'dir/file.txt']],
  ['unzip', ['-p', '/tmp/archive.zip', 'dir/file.txt']],
  ['bsdtar', ['-xOf', '/tmp/archive.tar', '--', 'dir/file.txt']],
  ['7z', ['e', '-so', '-bd', '--', '/tmp/archive.7z', 'dir/file.txt']],
  ['7zz', ['e', '-so', '-bd', '--', '/tmp/archive.7z', 'dir/file.txt']],
];

describe('validateCommand', () => {
  it.each(SAFE_ARCHIVE_BACKEND_COMMANDS)(
    'allows the fixed localBinaryInspect backend form for %s',
    (command, args) => {
      expect(validateCommand(command, args)).toEqual({ isValid: true });
    }
  );

  it('blocks archive/decompression backends when arguments contain shell metacharacters', () => {
    expect(validateCommand('tar', ['-xOf', '/tmp/archive.tar', '--', '$(id)']).isValid).toBe(false);
    expect(validateCommand('zcat', ['/tmp/$(id).gz']).isValid).toBe(false);
    expect(validateCommand('7z', ['e', '-so', '-bd', '--', '/tmp/archive.7z', 'a;rm']).isValid).toBe(false);
  });

  it('keeps dangerous backend flags blocked instead of allowing arbitrary command shapes', () => {
    expect(validateCommand('tar', ['-cf', '/tmp/out.tar', '/etc']).isValid).toBe(false);
    expect(validateCommand('unzip', ['-d', '/tmp/out', '/tmp/archive.zip']).isValid).toBe(false);
    expect(validateCommand('zstd', ['--rm', '/tmp/archive.zst']).isValid).toBe(false);
  });
});
