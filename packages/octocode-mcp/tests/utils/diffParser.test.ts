import { describe, it, expect } from 'vitest';
import { filterPatch, trimDiffContext } from '../../src/utils/parsers/diff.js';

describe('filterPatch', () => {
  const samplePatch = `@@ -1,3 +1,4 @@
 line 1
-deleted line
+added line 1
+added line 2
 line 3`;

  it('returns full patch when no filter arrays provided', () => {
    const result = filterPatch(samplePatch);
    expect(result).toBe(samplePatch);
  });

  it('returns full patch when both filters are undefined', () => {
    const result = filterPatch(samplePatch, undefined, undefined);
    expect(result).toBe(samplePatch);
  });

  it('returns empty when both filters are empty arrays', () => {
    const result = filterPatch(samplePatch, [], []);
    expect(result).toBe('');
  });

  it('filters to specific addition lines', () => {
    const result = filterPatch(samplePatch, [2], undefined);
    expect(result).toContain('+2: added line 1');
    expect(result).not.toContain('added line 2');
  });

  it('filters to specific deletion lines', () => {
    const result = filterPatch(samplePatch, undefined, [2]);
    expect(result).toContain('-2: deleted line');
  });

  it('returns empty string for empty patch input', () => {
    expect(filterPatch('')).toBe('');
  });
});

describe('trimDiffContext', () => {
  it('returns empty string for empty input', () => {
    expect(trimDiffContext('')).toBe('');
  });

  it('returns patch unchanged when ≤30 lines', () => {
    const short = Array.from({ length: 10 }, (_, i) => ` line ${i + 1}`).join(
      '\n'
    );
    expect(trimDiffContext(short)).toBe(short);
  });

  it('returns patch unchanged when no changed lines (+/-) are present', () => {
    const allContext = Array.from(
      { length: 35 },
      (_, i) => ` line ${i + 1}`
    ).join('\n');
    expect(trimDiffContext(allContext)).toBe(allContext);
  });

  it('trims excess context lines around changes in a long diff', () => {
    // 35 lines: 15 context, 1 addition, 19 more context
    const lines: string[] = [];
    for (let i = 0; i < 15; i++) lines.push(` ctx${i}`);
    lines.push('+added');
    for (let i = 0; i < 19; i++) lines.push(` ctx_after${i}`);
    const patch = lines.join('\n');

    const result = trimDiffContext(patch);
    // Result must be shorter than original (far-away context trimmed)
    expect(result.length).toBeLessThan(patch.length);
    // The changed line is retained
    expect(result).toContain('+added');
    // Lines within DIFF_CONTEXT_LINES=2 of change are kept (indices 13,14 = ctx13,ctx14)
    expect(result).toContain('ctx13');
    expect(result).toContain('ctx14');
    // Lines far from change become '...'
    expect(result).toContain('...');
  });

  it('preserves @@ hunk header lines unconditionally', () => {
    const lines: string[] = [];
    lines.push('@@ -1,20 +1,21 @@');
    for (let i = 0; i < 15; i++) lines.push(` ctx${i}`);
    lines.push('+added');
    for (let i = 0; i < 15; i++) lines.push(` after${i}`);
    const patch = lines.join('\n');

    const result = trimDiffContext(patch);
    expect(result).toContain('@@ -1,20 +1,21 @@');
  });

  it('returns original patch when trimmed version is not shorter', () => {
    // A diff where every line is changed — no context to trim,
    // so '...' lines are never inserted and trimmed equals original.
    const lines: string[] = [];
    for (let i = 0; i < 35; i++)
      lines.push(i % 2 === 0 ? `+add${i}` : `-del${i}`);
    const patch = lines.join('\n');

    const result = trimDiffContext(patch);
    expect(result).toBe(patch);
  });

  it('handles deletion lines the same as addition lines', () => {
    const lines: string[] = [];
    for (let i = 0; i < 18; i++) lines.push(` ctx${i}`);
    lines.push('-removed');
    for (let i = 0; i < 15; i++) lines.push(` after${i}`);
    const patch = lines.join('\n');

    const result = trimDiffContext(patch);
    expect(result).toContain('-removed');
    expect(result).toContain('...');
  });
});
