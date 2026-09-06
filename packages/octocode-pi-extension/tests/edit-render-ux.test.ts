import assert from 'node:assert/strict';
import { test } from 'vitest';

import { collapsedEditRationales } from '../src/tools/edit-render-ux.js';

test('collapsed multi-edit UI shows each rationale once without a Reasoning label', () => {
  const reasoning = 'Apply the same contract update to every branch.';
  const edits = Array.from({ length: 5 }, (_, editIndex) => ({
    editIndex,
    startLine: editIndex + 1,
    endLine: editIndex + 1,
    mode: 'exact' as const,
    reasoning,
    removedLines: [`old-${editIndex}`],
    addedLines: [`new-${editIndex}`],
  }));
  const rows = collapsedEditRationales([{ edits }]);

  assert.deepEqual(rows, [
    '(5 edits) Apply the same contract update to every branch.',
  ]);
  assert.doesNotMatch(rows.join('\n'), /Reasoning:/i);
});
