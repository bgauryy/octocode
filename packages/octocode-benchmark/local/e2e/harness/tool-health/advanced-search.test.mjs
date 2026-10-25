import assert from 'node:assert/strict';
import test from 'node:test';
import { advancedSearch } from './advanced-local/search.mjs';

test('multiline search records top-level execution errors instead of discarding the check history', async () => {
  const error = { error: 'captured multiline failure', errorCode: 'executionFailed' };
  const ctx = {
    surfaces: ['cli', 'mcp'],
    async explore(tool, input) {
      const json = input.queries[0].multiline === 'on' ? error : { results: [{ index: 0, data: {} }] };
      return { root: { json }, failures: [], responses: [{ tool, result: { json } }] };
    },
  };
  const checks = await advancedSearch(ctx);
  for (const surface of ctx.surfaces) {
    for (const engine of ['rust', 'pcre2']) {
      for (const view of ['content', 'files', 'countLines', 'countMatches', 'matchOnly']) {
        const prefix = `search.multiline-evidence.${engine}.${view}`;
        const complete = checks.find(([name]) => name === `${prefix}.${surface}.complete`);
        assert.equal(complete[1], false);
        assert.match(complete[2], /captured multiline failure/);
        assert.equal(checks.find(([name]) => name === `${prefix}.lines.${surface}`)[1], false);
      }
    }
    assert.ok(checks.some(([name]) => name === `search.content.${surface}`), 'earlier evidence checks are retained');
  }
});
