import { describe, expect, it } from 'vitest';
import {
  buildEvidenceMetadata,
  incompleteHintReasons,
} from '../../src/tools/evidence.js';

describe('tool evidence helpers', () => {
  it('builds complete evidence when data is answer-ready and no incomplete reasons exist', () => {
    expect(
      buildEvidenceMetadata({
        kind: 'code',
        answerReady: true,
        incompleteReasons: [],
        emptyReason: 'No matches.',
      })
    ).toEqual({
      kind: 'code',
      answerReady: true,
      complete: true,
    });
  });

  it('preserves the empty-result reason when no answer-ready data exists', () => {
    expect(
      buildEvidenceMetadata({
        kind: 'metadata',
        answerReady: false,
        incompleteReasons: [],
        emptyReason: 'No files matched.',
      })
    ).toEqual({
      kind: 'metadata',
      answerReady: false,
      complete: true,
      reason: 'No files matched.',
    });
  });

  it('dedupes and trims incomplete reasons before marking evidence incomplete', () => {
    expect(
      buildEvidenceMetadata({
        kind: 'structure',
        answerReady: true,
        incompleteReasons: [
          ' Entry pagination has more results. ',
          '',
          'Entry pagination has more results.',
          'Result hints report capped output.',
        ],
        emptyReason: 'No entries matched.',
      })
    ).toEqual({
      kind: 'structure',
      answerReady: true,
      complete: false,
      confidence: 'medium',
      reason:
        'Entry pagination has more results. Result hints report capped output.',
    });
  });

  it('detects capped, limited, and truncated hint words without matching unrelated words', () => {
    expect(
      incompleteHintReasons({
        hints: [
          'Results capped at 5 of 14.',
          'Results are unlimited in this directory.',
          'Skipped non-string hint below.',
          123,
        ],
      })
    ).toEqual(['Result hints report capped, limited, or truncated output.']);

    expect(
      incompleteHintReasons({
        hints: ['No limit was reached.', 'No truncation happened.'],
      })
    ).toEqual([]);
  });
});
