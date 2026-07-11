import { describe, expect, it } from 'vitest';

import { processFileContentAPI } from '../../src/github/fileContentProcess.js';

// Plain-ish source so "standard" minify visibly alters it (strips the
// comment) while remaining valid JS for the assertions below.
const SRC = '// license header\nfunction add(a, b) {\n  return a + b;\n}\n';

describe('ghGetFileContent — contentView is always surfaced', () => {
  it('minify:"none" reports contentView:"none"', async () => {
    const out = await processFileContentAPI(
      SRC,
      'octo',
      'engine',
      'main',
      'src/add.js',
      true,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'none'
    );
    expect(out.contentView).toBe('none');
  });

  it('minify:"standard" (the default) reports contentView:"standard", not silently omitted', async () => {
    const out = await processFileContentAPI(
      SRC,
      'octo',
      'engine',
      'main',
      'src/add.js',
      true,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'standard'
    );
    // Regression: this used to be omitted entirely for "standard" — the one
    // mode that both alters content AND wasn't marked, unlike none/symbols.
    expect(out.contentView).toBe('standard');
    expect(out.content).not.toBe(SRC);
  });
});

describe('ghGetFileContent — out-of-range line requests are signaled, not silently substituted', () => {
  const TWENTY_LINES = Array.from({ length: 20 }, (_, i) => `line ${i + 1}`).join(
    '\n'
  );

  it('an in-range startLine/endLine returns just that range with isPartial:true', async () => {
    const out = await processFileContentAPI(
      TWENTY_LINES,
      'octo',
      'engine',
      'main',
      'src/data.txt',
      false,
      5,
      10,
      0,
      undefined,
      undefined,
      undefined,
      'none'
    );
    expect(out.isPartial).toBe(true);
    expect(out.startLine).toBe(5);
    expect(out.endLine).toBe(10);
  });

  it('a startLine beyond the file length warns instead of silently returning the whole file unmarked', async () => {
    const out = await processFileContentAPI(
      TWENTY_LINES,
      'octo',
      'engine',
      'main',
      'src/data.txt',
      false,
      100,
      105,
      0,
      undefined,
      undefined,
      undefined,
      'none'
    );
    // Regression: this used to return the full file with isPartial left
    // undefined/false and no warning — indistinguishable from a genuinely
    // valid 100-105 request on a longer file.
    expect(out.isPartial).toBeFalsy();
    expect(out.warnings?.some((w) => w.includes('out of range'))).toBe(true);
  });

  it('endLine before startLine warns instead of silently returning the whole file unmarked', async () => {
    const out = await processFileContentAPI(
      TWENTY_LINES,
      'octo',
      'engine',
      'main',
      'src/data.txt',
      false,
      10,
      5,
      0,
      undefined,
      undefined,
      undefined,
      'none'
    );
    expect(out.isPartial).toBeFalsy();
    expect(out.warnings?.some((w) => w.includes('invalid'))).toBe(true);
  });
});
