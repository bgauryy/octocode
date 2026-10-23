// Guardrails measured after the workload: a stale target fails without retries, a source
// change invalidates its continuation, oversized rows stay complete and exact numeric
// predicates keep their lexemes. Writes guardrails.json under output.
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';
import { call, data, ok, queryAll, session } from './session.mjs';

export async function runGuardrails(output, port) {
  // Stale target guard: exact missing target must fail, with no mutation retries.
  session.phase = 'guardrails';
  session.current = null;
  const stale = data(
    await call('run', {
      connection: { port, target: 'missing-owned-target' },
      plan: {
        steps: [
          {
            op: 'cdp',
            method: 'Runtime.evaluate',
            params: { expression: '1' },
          },
        ],
      },
    })
  );
  assert.equal(stale.ok, false);
  const { oversize, changed } = await sourceGuards(output);
  const numeric = await exactNumericGuard(output);
  writeFileSync(
    resolve(output, 'guardrails.json'),
    JSON.stringify(
      {
        exactNumeric: numeric,
        staleTarget: stale,
        oversizedFirstPage: oversize,
        changedSource: changed,
        scope:
          'cancellation/queued/shutdown additionally measured by existing invocation unit tests',
      },
      null,
      2
    )
  );
}

// Source mutation invalidates returned continuation. Complete oversized evidence stays available.
async function sourceGuards(output) {
  const corpus = resolve(output, 'guard-corpus.json');
  writeFileSync(
    corpus,
    JSON.stringify([{ text: 'x'.repeat(20000) }, { text: 'boundary' }])
  );
  const oversize = ok(await call('query', { file: corpus, limit: 1 })).data;
  assert.equal(oversize.rows[0].oversized, true);
  let next = oversize.rows[0].next,
    oversizeContent = '';
  while (next) {
    const response = ok(await call(next.tool, next.query));
    oversizeContent += response.data.content;
    next = response.data?.next ?? null;
  }
  assert.equal(JSON.parse(oversizeContent).value.text.length, 20000);
  const oversizedRemainder = await queryAll(oversize.next.query);
  assert.equal(oversizedRemainder.rows.length, 1);
  assert.equal(oversizedRemainder.rows[0].text, 'boundary');
  writeFileSync(corpus, JSON.stringify([{ text: 'changed' }]));
  const changed = data(await call('query', oversize.next.query));
  assert.equal(changed.ok, false);
  return { oversize, changed };
}

async function exactNumericGuard(output) {
  const numericFile = resolve(output, 'numeric-corpus.json');
  writeFileSync(numericFile, '[{"n":9007199254740992},{"n":9007199254740993}]');
  const numeric = ok(
    await call('query', {
      args: [
        '--file',
        numericFile,
        '--where',
        '[{"path":"/n","op":"gte","value":9007199254740993}]',
      ],
    })
  ).data;
  assert.equal(numeric.scanned, 2);
  assert.equal(numeric.matched, 1);
  assert.equal(numeric.rows[0].sourceIndex, 1);
  return numeric;
}
