import { describe, expect, it } from 'vitest';

import { validateCommand } from '../../src/security/commandValidator.js';

const REMOVED_ARCHIVE_BACKEND_COMMANDS = [
  'file',
  'zcat',
  'gunzip',
  'bzcat',
  'xzcat',
  'zstdcat',
  'zstd',
  'lz4cat',
  'brotli',
  'lzfse',
  'tar',
  'unzip',
  'bsdtar',
  '7z',
  '7zz',
] as const;

describe('validateCommand', () => {
  it.each(REMOVED_ARCHIVE_BACKEND_COMMANDS)(
    'rejects removed archive/decompress backend command %s',
    command => {
      expect(validateCommand(command, ['/tmp/archive']).isValid).toBe(false);
      expect(validateCommand(command, ['/tmp/archive']).error).toMatch(/not allowed/);
    }
  );

  it('still allows core local commands', () => {
    expect(validateCommand('ls', ['-la', '/tmp']).isValid).toBe(true);
    expect(validateCommand('rg', ['pattern', '/tmp']).isValid).toBe(true);
  });
});
