// The optional scraping bridges refuse clearly when the octocode-scraping skill is not there.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { withHelpers } from './helper-fixture.mjs';

test('protocol-corpus, har-ingest and corpus-query name the missing skill and the flag to fix it', () =>
  withHelpers(async ({ call, put, cwd }) => {
    put('in.har', { log: { entries: [] } });
    const empty = join(cwd, 'no-skill-here');
    mkdirSync(empty);
    /** @type {[string, string[]][]} */
    const bridges = [
      ['protocol-corpus', ['--domains', 'Page']],
      ['har-ingest', ['--session-dir', cwd, '--har', 'in.har']],
      ['corpus-query', []],
    ];
    for (const [name, args] of bridges) {
      const none = await call(name, args);
      assert.ok(none.error, `${name} without the skill fails`);
      assert.match(none.error, /OPTIONAL_DEPENDENCY_MISSING/);
      assert.match(none.error, /--scraping-skill-dir/);
      const wrong = await call(name, ['--scraping-skill-dir', empty, ...args]);
      assert.ok(wrong.error, `${name} with an empty folder fails`);
      assert.match(wrong.error, /OPTIONAL_DEPENDENCY_MISSING/);
      assert.match(wrong.error, /No octocode-scraping skill at/);
      assert.ok(
        wrong.error.includes('no-skill-here'),
        'names the folder tried'
      );
    }
  }));
