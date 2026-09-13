import { describe, expect, it } from 'vitest';
import {
  compactIncomingCall,
  compactOutgoingCall,
  formatCallRow,
  formatLocationRow,
} from '../../../src/tools/lsp/semantic_content/semanticPresentation.js';

describe('lossless call-site presentation', () => {
  it('retains exact location endpoints in the compact view', () => {
    expect(
      formatLocationRow({
        uri: 'file:///project/unicode.ts',
        range: {
          start: { line: 1, character: 21 },
          end: { line: 1, character: 26 },
        },
        displayRange: { startLine: 1, endLine: 4 },
      })
    ).toBe('file:///project/unicode.ts:2:21-2:26');
  });

  const fromRanges = Array.from({ length: 12 }, (_, index) => ({
    start: { line: index, character: 2 },
    end: { line: index + 1, character: 8 },
  }));
  // Equal starts can have different ends; neither provider range may disappear.
  fromRanges.push({
    start: { line: 0, character: 2 },
    end: { line: 0, character: 9 },
  });
  const item = {
    name: 'helper',
    kind: 12,
    uri: 'file:///project/source.ts',
    range: fromRanges[0]!,
    selectionRange: fromRanges[0]!,
  };

  it.each(['incoming', 'outgoing'] as const)(
    'preserves all %s ranges and endpoints',
    direction => {
      const result =
        direction === 'incoming'
          ? compactIncomingCall({ direction, from: item, fromRanges }, 0)
          : compactOutgoingCall({ direction, to: item, fromRanges }, 0);
      const expected = fromRanges.map(range => ({
        line: range.start.line + 1,
        character: range.start.character,
        endLine: range.end.line + 1,
        endCharacter: range.end.character,
      }));

      expect(result.rangeCount).toBe(fromRanges.length);
      expect(result.ranges).toEqual(expected);
      const rendered = formatCallRow(result);
      for (const range of expected) {
        expect(rendered).toContain(
          `${range.line}:${range.character}-${range.endLine}:${range.endCharacter}`
        );
      }
    }
  );
});
