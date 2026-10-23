// Every MCP read tool: ordinary completeness plus four-reader context selection.
// No live provider: HTTP(S) is blocked except for the shared loopback fixture.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import {
  CASES,
  READ_TOOLS,
  materialize,
} from './read-tool-selection/cases.mjs';
import {
  ROOT,
  contract,
  dataOf,
  fieldsInEveryBranch,
  recoverUnfiltered,
  setup,
  startServer,
  walk,
} from './read-tool-selection/support.mjs';

const require = createRequire(import.meta.url);
const Ajv2020 = require('ajv/dist/2020').default;
const ajv = new Ajv2020({
  strict: false,
  allErrors: false,
  validateFormats: false,
});
const validators = new Map(
  contract.tools.map(tool => [tool.name, ajv.compile(tool.outputSchema)])
);
const schemasOnly = process.argv.includes('--schemas-only');
const selected = process.argv
  .slice(2)
  .find(arg => !arg.startsWith('--'))
  ?.split(',');
const cases = selected
  ? CASES.filter(row => selected.includes(row.tool))
  : CASES;
assert.deepEqual([...READ_TOOLS].sort(), contract.tools.filter(t => !t.beta && !t.cliOnly).map(t => t.name).sort(), 'one case per published (non-beta, MCP) contract tool');
assert.deepEqual(
  CASES.filter(row => row.scope)
    .map(row => row.tool)
    .sort(),
  ['ghGetFileContent', 'ghSearchCode', 'localFetch', 'localSearch']
);
if (selected)
  assert.deepEqual(
    [...cases.map(row => row.tool)].sort(),
    [...selected].sort(),
    'unknown tool selection'
  );

const ctx = await setup();
const checks = [],
  tools = [],
  raw = [],
  schemas = [];
const output = path.join(
  ROOT,
  'packages/octocode-benchmark/local/e2e/results/read-tool-selection',
  ...(schemasOnly ? ['schema-recheck'] : [])
);
fs.mkdirSync(output, { recursive: true });
let mcp;

async function check(tool, name, run) {
  const before = ctx.calls(),
    started = performance.now();
  try {
    const detail = await run();
    checks.push({
      tool,
      name,
      status: detail?.skip ? 'SKIP' : 'PASS',
      detail,
      providerCalls: ctx.calls() - before,
      ms: performance.now() - started,
    });
    console.log(
      `${detail?.skip ? 'SKIP' : 'PASS'} ${tool}: ${name}${detail?.skip ? ` — ${detail.skip}` : ''}`
    );
    return detail;
  } catch (error) {
    checks.push({
      tool,
      name,
      status: 'FAIL',
      error: error.stack,
      providerCalls: ctx.calls() - before,
      ms: performance.now() - started,
    });
    console.log(`FAIL ${tool}: ${name} — ${error.message.slice(0, 400)}`);
  } finally {
    ctx.state.mode = 'score';
  }
}

const sorted = rows => [...rows].sort();
const expectedItems = {
  localFetch: 80,
  structureSearch: 7,
  astSearch: 10,
  ghSearchRepo: 3,
  ghSearchCode: 7,
  ghGetFileContent: 80,
  ghSearchHistory: 2,
  artifactSearch: 3,
};
const contextFor = (spec, extra = {}) => ({
  clasifyContext: { classifiers: spec.classifiers, ...extra },
  mainGoal:
    'Explain cancellation and configured greeting behavior from current source evidence.',
  reasoning:
    'Select direct implementation evidence before returning source bodies.',
});

function resolveLocalRef(root, value) {
  const prefix = '#/$defs/';
  return value?.$ref?.startsWith(prefix)
    ? root.$defs?.[value.$ref.slice(prefix.length)]
    : value;
}

function assertSelectionFields(schema, spec, shown, opaque = shown) {
  const branches = schema.oneOf ?? schema.anyOf ?? [schema];
  for (const branch of branches) {
    const eligible = spec.scope !== null;
    assert.equal(
      fieldsInEveryBranch(branch, 'clasifyContext'),
      shown && eligible,
      `${spec.tool} clasifyContext on ${branch.properties?.operation?.const ?? 'query'}`
    );
    assert.equal(
      fieldsInEveryBranch(branch, 'selection'),
      opaque && eligible,
      `${spec.tool} selection on ${branch.properties?.operation?.const ?? 'query'}`
    );
    assert.equal(
      fieldsInEveryBranch(branch, 'classifiers'),
      false,
      `${spec.tool} has no legacy classifiers input`
    );
    assert.equal(
      fieldsInEveryBranch(branch, 'ranking'),
      false,
      `${spec.tool} has no legacy ranking input`
    );
    if (shown && eligible) {
      const context = resolveLocalRef(
        schema,
        branch.properties?.clasifyContext
      );
      assert.ok(
        context?.properties?.classifiers,
        `${spec.tool} context owns classifiers`
      );
      assert.ok(
        context?.properties?.longContext,
        `${spec.tool} context owns longContext`
      );
    }
  }
}

function valid(spec, run) {
  for (const response of run.responses) {
    const validate = validators.get(spec.tool);
    assert.ok(
      validate(response.json),
      `${spec.tool} output: ${JSON.stringify(validate.errors)}`
    );
  }
}

function equalEvidence(left, right, message) {
  assert.deepEqual(sorted(left.rows), sorted(right.rows), message);
}

function independentEvidence(spec, run, query) {
  if (['localFetch', 'ghGetFileContent'].includes(spec.tool)) {
    const source =
      spec.tool === 'localFetch'
        ? fs.readFileSync(path.resolve(ctx.fixture, query.path), 'utf8')
        : ctx.mock.model.files[query.path];
    assert.equal(
      typeof source,
      'string',
      'the independent fixture owns the complete source'
    );
    const expected = source
      .replace(/\n$/, '')
      .split('\n')
      .map((line, index) => `${index + 1}\t${line}`);
    const actual = run.rows.map(row => JSON.parse(row).line);
    assert.deepEqual(
      sorted(actual),
      sorted(expected),
      'ordinary traversal returns every source line exactly once'
    );
  } else if (['localSearch', 'ghSearchCode'].includes(spec.tool)) {
    const terms =
      spec.tool === 'localSearch' ? [query.matchString] : query.keywords;
    const expected = Object.entries(ctx.mock.model.files)
      .filter(
        ([file, text]) =>
          (!query.path ||
            file.startsWith(`${query.path}/`) ||
            file === query.path) &&
          terms.every(term => text.toLowerCase().includes(term.toLowerCase()))
      )
      .map(([file]) => file);
    const actual = run.rows.flatMap(row => {
      const file = JSON.parse(row);
      return [file.path, ...(file.alsoAt ?? [])];
    });
    if (spec.tool === 'localSearch') {
      assert.deepEqual(sorted([...new Set(actual)]), sorted(expected),
        'ordinary traversal reaches every matching file');
      const expectedMatches = Object.entries(ctx.mock.model.files).flatMap(([file, text]) =>
        expected.includes(file) ? text.split('\n').flatMap((line, index) =>
          terms.every(term => line.toLowerCase().includes(term.toLowerCase()))
            ? [`${file}:${index + 1}`] : []) : []);
      const actualMatches = run.rows.map(row => {
        const file = JSON.parse(row);
        return `${file.path}:${file.match.line}`;
      });
      assert.deepEqual(sorted(actualMatches), sorted(expectedMatches),
        'ordinary traversal returns every match exactly once');
    } else {
      assert.deepEqual(sorted(actual), sorted(expected),
        'ordinary traversal returns every matching file exactly once');
    }
  }
}

function receipts(spec, run) {
  return run.responses.map(response => {
    const receipt = dataOf(response).selection;
    assert.ok(receipt, 'classified output always includes a selection receipt');
    assert.ok(
      ['complete', 'partial', 'unavailable'].includes(receipt.status),
      `selection status ${receipt.status}`
    );
    assert.equal(receipt.scope, spec.scope);
    assert.equal(receipt.unit, spec.unit);
    for (const field of [
      'totalItems',
      'selectedItems',
      'excludedItems',
      'unknownItems',
    ]) {
      assert.ok(
        Number.isInteger(receipt[field]) && receipt[field] >= 0,
        `${field} is a nonnegative integer`
      );
    }
    assert.ok(receipt.relatedItems === undefined || (Number.isInteger(receipt.relatedItems) && receipt.relatedItems > 0), 'relatedItems (decided near-misses, hidden) is omitted or positive');
    assert.equal(
      receipt.selectedItems + receipt.excludedItems + (receipt.relatedItems ?? 0) + receipt.unknownItems,
      receipt.totalItems,
      'selection partitions reconcile'
    );
    return receipt;
  });
}

function assertSelectedOnly(spec, run) {
  const body = run.rows.join('\n');
  assert.ok(body.includes(spec.target), 'selected evidence is returned');
  assert.ok(
    !body.includes('UNKNOWN_SELECTION_SENTINEL'),
    'unknown evidence body stays hidden'
  );
  assert.ok(
    !body.includes('EXCLUDED_SELECTION_SENTINEL'),
    'excluded evidence body stays hidden'
  );
  const rs = receipts(spec, run);
  assert.ok(
    rs.some(row => row.selectedItems > 0),
    'receipt counts selected evidence'
  );
  return rs;
}

function assertSourceOrder(selectedRun, ordinaryRun) {
  let cursor = 0;
  for (const row of selectedRun.rows) {
    cursor = ordinaryRun.rows.indexOf(row, cursor);
    assert.notEqual(
      cursor,
      -1,
      'selected evidence preserves ordinary source order'
    );
    cursor += 1;
  }
}

async function assertRecovery(spec, classified, baseline, options = {}) {
  const recovered = await recoverUnfiltered(
    ctx,
    spec,
    classified.responses[0],
    options
  );
  valid(spec, recovered);
  if (spec.scope === 'selectedView')
    equalEvidence(
      recovered,
      baseline,
      'unfiltered recovery restores the complete original view'
    );
  else {
    assert.ok(
      recovered.rows.length,
      'provider-page recovery returns unfiltered evidence'
    );
    assert.ok(
      recovered.rows.length >=
        dataOf(classified.responses[0]).selection.totalItems,
      'provider-page recovery covers the classified page without claiming an immutable later page'
    );
  }
  assert.ok(
    recovered.responses.every(
      response => dataOf(response).selection === undefined
    ),
    'recovery is an ordinary projection'
  );
  return recovered;
}

try {
  for (const [label, overrides, shown] of [
    ['enabled', {}, true],
    ['missing provider key', { OCTOCODE_CLASSIFICATION_API: '' }, false],
    ['public clasify hidden', { DISABLE_TOOLS: 'clasify' }, true],
    ['beta off', { OCTOCODE_BETA: 'false' }, true],
    [
      'removed ranking gate ignored',
      { OCTOCODE_RANKING_ENABLED: 'false' },
      true,
    ],
  ]) {
    await check(
      'all13',
      `CLI/MCP selection schema gating: ${label}`,
      async () => {
        const server = await startServer({
          env: { ...ctx.env, ...overrides },
          cwd: ctx.fixture,
          timeoutMs: 60000,
        });
        try {
          schemas.push({
            label,
            surface: 'mcp.tools/list',
            payload: { tools: server.tools },
            rawResponse: `${JSON.stringify({ tools: server.tools })}\n`,
          });
          for (const spec of cases) {
            const contractTool = contract.tools.find(
              row => row.name === spec.tool
            );
            const tool = server.tools.find(row => row.name === spec.tool);
            if (contractTool.cliOnly)
              assert.equal(tool, undefined, `${spec.tool} remains CLI-only`);
            else {
              assert.ok(tool, `${spec.tool} is exposed`);
              assertSelectionFields(
                tool.inputSchema.properties.queries.items,
                spec,
                shown,
                false
              );
            }
            const full = await ctx.cliArgs(
              ['schema', spec.tool, '--view', 'full'],
              overrides
            );
            assert.ok(full.json.querySchema, `${spec.tool} CLI schema`);
            schemas.push({
              label,
              tool: spec.tool,
              surface: 'cli.schema.full',
              rawResponse: full.rawResponse,
              payload: full.json,
            });
            assertSelectionFields(full.json.querySchema, spec, shown);
          }
          return { tools: cases.map(row => row.tool), shown };
        } finally {
          server.close();
        }
      }
    );
  }

  await check('all13', 'discovery never contacts the provider', async () => {
    ctx.state.mode = '401';
    const before = ctx.calls();
    const server = await startServer({
      env: ctx.env,
      cwd: ctx.fixture,
      timeoutMs: 60000,
    });
    try {
      for (const spec of cases.filter(
        spec => !contract.tools.find(row => row.name === spec.tool).cliOnly
      )) {
        const schema = server.tools.find(row => row.name === spec.tool)
          ?.inputSchema.properties.queries.items;
        assert.ok(schema);
        assertSelectionFields(schema, spec, true, false);
      }
      assert.equal(ctx.calls(), before, 'tools/list sends no provider request');
    } finally {
      server.close();
    }
  });

  mcp = await startServer({ env: ctx.env, cwd: ctx.fixture, timeoutMs: 60000 });
  await check(
    'all13',
    'legacy top-level ranking inputs are rejected by CLI and MCP',
    async () => {
      const spec = CASES.find(row => row.scope);
      const plain = materialize(spec.query, ctx);
      const before = ctx.calls();
      for (const field of [
        { classifiers: spec.classifiers },
        { ranking: 'legacy-plan' },
      ]) {
        const cli = await ctx.cli(spec.tool, { ...plain, ...field });
        assert.equal(cli.code, 2);
        assert.equal(
          cli.json.errorCode ?? dataOf(cli)?.errorCode,
          'invalidInput'
        );
        const throughMcp = await mcp.raw(spec.tool, {
          queries: [{ ...plain, ...field }],
        });
        assert.equal(throughMcp.isError, true);
        assert.equal(throughMcp.sc?.errorCode, 'invalidInput');
      }
      assert.equal(
        ctx.calls(),
        before,
        'legacy aliases never reach the provider'
      );
    }
  );

  for (const spec of schemasOnly ? [] : cases) {
    const plain = materialize(spec.query, ctx);
    if (spec.scope === null) {
      await check(
        spec.tool,
        'ordinary evidence stays complete and selection controls are rejected',
        async () => {
          const before = ctx.calls();
          const baseline = await walk(ctx, spec, plain);
          valid(spec, baseline);
          assert.ok(baseline.rows.length);
          if (expectedItems[spec.tool])
            assert.equal(baseline.rows.length, expectedItems[spec.tool]);
          assert.ok(
            baseline.responses.every(
              response => dataOf(response).selection === undefined
            )
          );
          const cliOnly = contract.tools.find(
            row => row.name === spec.tool
          ).cliOnly;
          if (!cliOnly) {
            const throughMcp = await walk(ctx, spec, plain, {
              surface: 'mcp',
              mcp,
            });
            valid(spec, throughMcp);
            equalEvidence(
              throughMcp,
              baseline,
              'MCP and CLI preserve ordinary evidence'
            );
          }
          for (const field of [
            { clasifyContext: { classifiers: spec.classifiers } },
            { selection: 'unsupported-plan' },
          ]) {
            const rejected = await ctx.cli(spec.tool, { ...plain, ...field });
            assert.equal(rejected.code, 2);
            assert.equal(
              rejected.json.errorCode ?? dataOf(rejected)?.errorCode,
              'invalidInput'
            );
          }
          assert.equal(
            ctx.calls(),
            before,
            'ordinary and unsupported routes never classify'
          );
          tools.push({
            tool: spec.tool,
            scope: null,
            status: 'unsupported',
            pages: baseline.responses.length,
            evidenceItems: baseline.rows.length,
            providerCalls: 0,
          });
        }
      );
      continue;
    }

    const classifiedQuery = { ...plain, ...contextFor(spec) };
    let baseline, classified;
    await check(
      spec.tool,
      'ordinary traversal remains complete and performs zero JEV work',
      async () => {
        const before = ctx.calls();
        baseline = await walk(ctx, spec, plain);
        valid(spec, baseline);
        independentEvidence(spec, baseline, plain);
        if (expectedItems[spec.tool])
          assert.equal(baseline.rows.length, expectedItems[spec.tool]);
        assert.ok(
          baseline.responses.every(
            response => dataOf(response).selection === undefined
          )
        );
        assert.equal(ctx.calls(), before);
        return {
          pages: baseline.responses.length,
          evidenceItems: baseline.rows.length,
        };
      }
    );

    await check(
      spec.tool,
      'CLI returns selected bodies only with reconciled receipts and lossless recovery',
      async () => {
        classified = await walk(ctx, spec, classifiedQuery);
        valid(spec, classified);
        const rs = assertSelectedOnly(spec, classified);
        assert.ok(
          rs.some(row => row.excludedItems > 0),
          'negative sentinel is counted as excluded'
        );
        assert.ok(
          rs.some(row => row.unknownItems > 0),
          'ambiguous sentinel is counted as unknown'
        );
        assertSourceOrder(classified, baseline);
        if (spec.scope === 'selectedView')
          assert.ok(
            classified.provider.slice(1).every(calls => calls === 0),
            'selected-view pages replay without new judgments'
          );
        await assertRecovery(spec, classified, baseline);
        tools.push({
          tool: spec.tool,
          scope: spec.scope,
          status: rs[0].status,
          pages: classified.responses.length,
          evidenceItems: classified.rows.length,
          providerCalls: classified.providerCalls,
        });
        raw.push({ tool: spec.tool, baseline, classified });
        return rs[0];
      }
    );
    if (!baseline || !classified) continue;

    await check(
      spec.tool,
      'fresh MCP returns the same selected projection and supports unfiltered recovery',
      async () => {
        const throughMcp = await walk(ctx, spec, classifiedQuery, {
          surface: 'mcp',
          mcp,
        });
        valid(spec, throughMcp);
        assertSelectedOnly(spec, throughMcp);
        assert.deepEqual(
          throughMcp.rows,
          classified.rows,
          'MCP and CLI selected evidence agree'
        );
        await assertRecovery(spec, throughMcp, baseline, {
          surface: 'mcp',
          mcp,
        });
        raw.push({ tool: spec.tool, mcp: throughMcp });
      }
    );

    await check(
      spec.tool,
      'one candidate is judged instead of bypassing selection',
      async () => {
        const query = { ...materialize(spec.single, ctx), ...contextFor(spec) };
        const before = ctx.calls();
        const run = await walk(ctx, spec, query);
        const rs = receipts(spec, run);
        assert.equal(rs[0].totalItems, 1);
        assert.equal(rs[0].selectedItems, 1);
        assert.ok(ctx.calls() > before, 'single candidate reaches JEV');
        assertSelectedOnly(spec, run);
      }
    );

    await check(
      spec.tool,
      'provider failure exposes no unjudged body and next.unfiltered recovers without JEV',
      async () => {
        ctx.state.mode = '401';
        const failed = await walk(ctx, spec, classifiedQuery, {
          errorCode: 'classificationProviderError',
        });
        const rs = receipts(spec, failed);
        assert.ok(
          rs.some(row => ['partial', 'unavailable'].includes(row.status))
        );
        assert.equal(
          failed.rows.length,
          0,
          'failed judgments do not leak raw candidate bodies'
        );
        assert.ok(rs.some(row => row.unknownItems > 0));
        await assertRecovery(spec, failed, baseline);
      }
    );

    // Candidate and character budgets are runtime constants: the unjudged
    // tail stays unknown and recoverable in selection::tests
    // (candidate_budget_counts_serialized_characters_in_source_order,
    // partial_receipts_distinguish_ineligible_evidence_from_budgets) and
    // local_fetch::selection::tests::exhausted_state_budget_withholds_bodies_and_unfiltered_recovers_every_line;
    // the per-tool unknown-tail recovery path runs in the provider-failure case above.

    await check(
      spec.tool,
      'incompatible views fail before provider work',
      async () => {
        for (const query of [...(spec.protected ?? [])]) {
          const before = ctx.calls();
          const rejected = await ctx.cli(spec.tool, {
            ...materialize(query, ctx),
            ...contextFor(spec),
          });
          assert.equal(rejected.code, 2);
          assert.equal(
            rejected.json.errorCode ?? dataOf(rejected)?.errorCode,
            'invalidInput'
          );
          assert.equal(ctx.calls(), before);
        }
      }
    );

    await check(
      spec.tool,
      'selection continuations reject changed context and malformed plans',
      async () => {
        const continued = classified.edges.filter(
          edge => edge.kind !== 'unfiltered'
        );
        for (const edge of continued) {
          const query = edge.query?.queries?.[0];
          assert.ok(
            query?.mainGoal?.trim(),
            `${edge.kind} preserves nonblank mainGoal`
          );
          assert.ok(
            query?.reasoning?.trim(),
            `${edge.kind} preserves nonblank reasoning`
          );
          assert.deepEqual(
            query?.clasifyContext,
            classifiedQuery.clasifyContext,
            `${edge.kind} preserves clasifyContext`
          );
          if (spec.scope === 'providerPage' && !query.selection)
            assert.ok(
              edge.kind === 'nextPage' || edge.kind === 'nextMatchPage',
              'fresh provider-page lead may omit selection only on a provider continuation'
            );
        }
        const lead = classified.edges.find(
          edge => edge.query?.queries?.[0]?.selection
        );
        if (!lead && spec.scope === 'providerPage') {
          assert.ok(
            continued.some(
              edge =>
                (edge.kind === 'nextPage' || edge.kind === 'nextMatchPage') &&
                !edge.query?.queries?.[0]?.selection
            ),
            'fresh provider continuation retains context without replaying a page-bound plan'
          );
          return { freshProviderPage: true };
        }
        if (!lead)
          return { skip: 'fixture produced no selected-output continuation' };
        const row = lead.query.queries[0];
        for (const change of [
          { selection: 'malformed-plan' },
          {
            clasifyContext: {
              classifiers: ['A different atomic evidence question.'],
            },
          },
          { mainGoal: 'A changed goal invalidates the saved plan.' },
          {
            reasoning:
              'A changed evidence rationale invalidates the saved plan.',
          },
        ]) {
          const before = ctx.calls();
          const replay = await ctx.cli(spec.tool, { ...row, ...change });
          assert.equal(
            dataOf(replay)?.errorCode ?? replay.json.errorCode,
            'staleSnapshot'
          );
          assert.equal(
            ctx.calls(),
            before,
            'invalid replay cannot spend provider work'
          );
        }
      }
    );

    if (spec.tool.startsWith('local'))
      await check(
        spec.tool,
        'selected plan replay rejects source digest drift',
        async () => {
          const lead = classified.edges.find(
            edge => edge.query?.queries?.[0]?.selection
          );
          if (!lead)
            return { skip: 'fixture produced no selected-output continuation' };
          const relative =
            spec.tool === 'localFetch'
              ? plain.path
              : 'src/selected-selection.ts';
          const file = path.resolve(ctx.fixture, relative);
          const original = fs.readFileSync(file);
          const before = ctx.calls();
          try {
            fs.appendFileSync(file, '\n// source digest drift\n');
            const replay = await ctx.cli(spec.tool, lead.query.queries[0]);
            assert.equal(
              dataOf(replay)?.errorCode ?? replay.json.errorCode,
              'staleSnapshot'
            );
            assert.equal(
              ctx.calls(),
              before,
              'stale replay cannot spend provider work'
            );
          } finally {
            fs.writeFileSync(file, original);
          }
        }
      );

    await check(
      spec.tool,
      'selection does not hydrate hidden bodies or send routing metadata',
      async () => {
        const packets = ctx.mock.classificationRequests.length;
        await ctx.cli(spec.tool, classifiedQuery);
        for (const packet of ctx.mock.classificationRequests.slice(packets)) {
          const serialized = JSON.stringify(packet.state);
          assert.ok(!serialized.includes('BODY_ONLY_SENTINEL'));
          assert.ok(
            !serialized.includes(ctx.fixture),
            'absolute local workspace prefix stays private'
          );
          const inspect = node => {
            if (!node || typeof node !== 'object') return;
            for (const [key, value] of Object.entries(node)) {
              assert.ok(
                !['next', 'hints', 'selection', 'snapshot'].includes(key),
                `provider evidence contains routing metadata: ${key}`
              );
              inspect(value);
            }
          };
          inspect(packet.state);
        }
      }
    );
  }

  if (!schemasOnly) {
    const spec = CASES.find(row => row.tool === 'localFetch');
    const plain = materialize(spec.single, ctx);
    await check(
      'localFetch',
      'optional metadata reaches JEV and longContext sets the provider header',
      async () => {
        const packetStart = ctx.mock.classificationRequests.length;
        const headerStart = ctx.classificationHeaders.length;
        const query = {
          ...plain,
          ...contextFor(spec, { longContext: true }),
          mainGoal: 'OPTIONAL_MAIN_GOAL_SENTINEL',
          reasoning: 'OPTIONAL_REASONING_SENTINEL',
        };
        const run = await walk(ctx, spec, query);
        assertSelectedOnly(spec, run);
        const packets = ctx.mock.classificationRequests.slice(packetStart);
        assert.ok(packets.length);
        const serialized = JSON.stringify(packets);
        assert.ok(serialized.includes('OPTIONAL_MAIN_GOAL_SENTINEL'));
        assert.ok(serialized.includes('OPTIONAL_REASONING_SENTINEL'));
        const headers = ctx.classificationHeaders.slice(headerStart);
        assert.ok(
          headers.every(
            row => row['typesafe-danger-zone-long-context'] === 'true'
          )
        );
      }
    );

    // The selection deadline is a runtime constant: a read past it discloses
    // the gap and recovers in runtime_clasify::a_selection_past_its_deadline_withholds_bodies_and_recovers.

    await check(
      'localFetch',
      'the declared probability policy selects, excludes and preserves unknown',
      async () => {
        const query = {
          ...plain,
          clasifyContext: {
            classifiers: [
              'Does this directly state the cancellation rule?',
              'Does this provide related cancellation evidence?',
            ],
          },
        };
        for (const [mode, partition] of [
          ['policyAnyDirect', 'selectedItems'],
          ['policyAllNo', 'excludedItems'],
          ['policyUnknown', 'unknownItems'],
          ['invalidContradiction', 'unknownItems'],
        ]) {
          ctx.state.mode = mode;
          const run = await walk(ctx, spec, query, mode === 'invalidContradiction'
            ? { errorCode: 'invalidClassificationResponse' } : {});
          const receipt = receipts(spec, run)[0];
          assert.equal(receipt.totalItems, 1);
          assert.equal(receipt[partition], 1, `${mode} maps to ${partition}`);
          assert.equal(
            run.rows.length > 0,
            partition === 'selectedItems',
            'only selected candidates expose a body'
          );
        }
      }
    );

    await check(
      'localSearch',
      'arbitrary source ordering does not disable selection',
      async () => {
        const local = CASES.find(row => row.tool === 'localSearch');
        for (const ordering of [{ sort: 'path' }, { reverse: true }]) {
          const run = await walk(ctx, local, {
            ...materialize(local.query, ctx),
            ...ordering,
            ...contextFor(local),
          });
          assertSelectedOnly(local, run);
          assert.ok(run.providerCalls > 0);
        }
      }
    );

    await check(
      'localFetch',
      'missing capability rejects explicit selection without provider access',
      async () => {
        const before = ctx.calls();
        const rejected = await ctx.cli(
          'localFetch',
          { ...plain, ...contextFor(spec) },
          { OCTOCODE_CLASSIFICATION_API: '' }
        );
        assert.equal(rejected.code, 2);
        assert.equal(
          rejected.json.errorCode ?? dataOf(rejected)?.errorCode,
          'invalidInput'
        );
        assert.equal(ctx.calls(), before);
      }
    );
  }
} finally {
  mcp?.close();
  fs.writeFileSync(
    path.join(output, 'raw.json'),
    JSON.stringify(
      {
        responses: raw,
        cliTrace: ctx.trace,
        providerPackets: ctx.mock.classificationRequests,
        providerHeaders: ctx.classificationHeaders,
        upstream: ctx.mock.requests,
      },
      null,
      2
    )
  );
  fs.writeFileSync(
    path.join(output, 'schemas.json'),
    JSON.stringify(schemas, null, 2)
  );
  fs.writeFileSync(
    path.join(output, 'results.json'),
    JSON.stringify(
      {
        at: new Date().toISOString(),
        contract: contract.fingerprint,
        expectedTools: READ_TOOLS,
        schemasOnly,
        selectedTools: cases.map(row => row.tool),
        tools,
        checks,
        failed: checks.filter(row => row.status === 'FAIL').length,
        skipped: checks.filter(row => row.status === 'SKIP').length,
      },
      null,
      2
    )
  );
  await ctx.close();
}

const failed = checks.filter(row => row.status === 'FAIL');
console.log(
  `${checks.length - failed.length}/${checks.length} read-tool selection checks passed or explicitly skipped`
);
if (failed.length) process.exitCode = 1;
