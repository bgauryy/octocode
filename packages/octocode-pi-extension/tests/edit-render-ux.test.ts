import assert from 'node:assert/strict';
import { test } from 'vitest';

import { renderEditResult } from '../src/tools/edit-tool.js';

test('collapsed multi-edit UI groups identical reasoning instead of repeating it', () => {
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
  const rendered = renderEditResult({
    content: [{ type: 'text', text: 'edited' }],
    details: { replacements: 5, files: [{ path: '/workspace/example.ts', edits }] },
  }, { expanded: false }, undefined, 'file (Octocode)').render(240).join('\n');

  assert.equal(rendered.match(/Reasoning:/g)?.length, 1);
  assert.match(rendered, /Reasoning: \(5 edits\) Apply the same contract update/);
  assert.match(rendered, /2 more reasoning\/diff lines hidden/i, 'deduplication exposes four additional diff lines');
});
