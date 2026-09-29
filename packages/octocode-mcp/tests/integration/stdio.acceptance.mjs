/** Real built-server acceptance. Run after building CLI + MCP; no mocks or installs. */
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { z } from 'zod';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { createLocalAcceptanceFixture } from './local-acceptance-fixture.mjs';

const { values } = parseArgs({
  options: {
    server: { type: 'string', default: 'packages/octocode-mcp/dist/index.js' },
    cli: { type: 'string', default: 'packages/octocode/out/octocode.js' },
    cwd: { type: 'string', default: process.cwd() },
    node: { type: 'string', default: process.execPath },
    fixture: { type: 'string' },
    receipt: {
      type: 'string',
      default: '.octocode/octocode-research/preproduction-mcp-receipt.json',
    },
    quick: { type: 'boolean', default: false },
    live: { type: 'boolean', default: false },
    'cli-mcp-parity': { type: 'boolean', default: false },
  },
});
const fixture = values.fixture
  ? path.resolve(values.fixture)
  : await createLocalAcceptanceFixture(path.resolve('.octocode/tmp'));
const acceptanceCwd = path.resolve(values.cwd);
const acceptanceEnv = {
  ...process.env,
  ENABLE_LOCAL: 'true',
  OCTOCODE_BETA: 'true',
  OCTOCODE_STORAGE_MODE: 'persistent',
};
const { DIRECT_TOOL_DEFINITIONS, TOOL_NAMES, getDirectToolDefinitionsWithAddons } = await import('@octocodeai/config/schema');
const canonicalTools = DIRECT_TOOL_DEFINITIONS.map(tool => tool.name);
// Mutating tools run only from the CLI; MCP never lists or executes them.
const CLI_ONLY_TOOLS = new Set(['ghCloneRepo', 'astRewrite']);
let expectedTools = [];
const receipt = {
  server: path.resolve(values.server),
  node: values.node,
  fixture,
  checks: [],
  calls: [],
  transportErrors: [],
  stderrBytes: 0,
  stderrTail: '',
  guidance: {
    recoveryHints: 0,
    maxHintChars: 0,
    continuations: 0,
    validationErrors: 0,
  },
};
const cliMcpParitySamples = new Map();
const cliCalls = [];
const suiteStartedAt = performance.now();
const transport = new StdioClientTransport({
  command: values.node,
  args: [path.resolve(values.server)],
  cwd: acceptanceCwd,
  env: acceptanceEnv,
  stderr: 'pipe',
});
const client = new Client({
  name: 'octocode-stdio-acceptance',
  version: '1.0.0',
});
client.onerror = error => receipt.transportErrors.push(error.name);
const check = async (name, fn) => {
  try {
    await fn();
    receipt.checks.push({ name, status: 'passed' });
  } catch (error) {
    receipt.checks.push({ name, status: 'failed', error: error.message });
  }
};
// Every tool query requires `goal` and `reasoning`; fixtures state only what
// they exercise, so default both here. Explicit values always win.
const TRACE = 'Exercise the built MCP surface in stdio acceptance.';
const withTrace = queries =>
  queries.map(query =>
    query && typeof query === 'object' && !Array.isArray(query)
      ? { goal: TRACE, reasoning: TRACE, ...query }
      : query
  );
const invoke = async (name, rawArgs) => {
  const args = Array.isArray(rawArgs?.queries)
    ? { ...rawArgs, queries: withTrace(rawArgs.queries) }
    : rawArgs;
  const startedAt = performance.now();
  const response = await client.callTool({ name, arguments: args });
  const durationMs = Number((performance.now() - startedAt).toFixed(2));
  const responseBytes = Buffer.byteLength(JSON.stringify(response));
  receipt.calls.push({ name, arguments: args, durationMs, responseBytes, response });
  for (const row of response.structuredContent?.results ?? []) {
    const recovery = row.status === 'empty' || row.status === 'error';
    let hintCount = 0;
    const inspect = value => {
      if (!value || typeof value !== 'object') return;
      for (const [key, child] of Object.entries(value)) {
        if (['query', 'content', 'body', 'patch', 'text', 'value', 'matches'].includes(key)) continue;
        if (key === 'hints') {
          assert.ok(recovery, `${name}: hints on a successful result`);
          hintCount += child.length;
          assert.ok(child.every(hint => hint.length <= 160), `${name}: long hint`);
          receipt.guidance.recoveryHints += child.length;
          receipt.guidance.maxHintChars = Math.max(
            receipt.guidance.maxHintChars,
            ...child.map(hint => hint.length)
          );
        }
        if (key === 'next') {
          for (const [nextKey, call] of Object.entries(child)) {
            assert.ok(!['fetch', 'getLines', 'readSite', 'viewTree', 'viewStructure', 'cloneRepo', 'searchRepositoryCode', 'lspDefinition', 'lspReferences'].includes(nextKey), `${name}: unsolicited ${nextKey}`);
            assert.ok(expectedTools.includes(call?.tool), `${name}: continuation ${nextKey} has no runnable tool`);
            assert.ok(call?.query && typeof call.query === 'object', `${name}: continuation ${nextKey} has no executable query`);
            if (!recovery) assert.equal(call.why, undefined, `${name}: success continuation prose`);
            receipt.guidance.continuations += 1;
          }
        }
        inspect(child);
      }
    };
    inspect(row);
    assert.ok(hintCount <= 2, `${name}: too many recovery hints`);
  }
  return response;
};
const call = async (name, query) => {
  const response = await invoke(name, { queries: [query] });
  assert.equal(response.isError, false, `${name} returned a tool error`);
  assert.ok(response.structuredContent, `${name} has no structured content`);
  assert.ok(
    response.content.some(block => block.type === 'text' && block.text.length),
    `${name} has no text representation`
  );
  const row = response.structuredContent.results?.[0];
  assert.ok(row?.data, `${name} has no result data`);
  assert.notEqual(row.status, 'error', `${name} result failed`);
  assert.equal(row.data.error, undefined, `${name} returned an error payload`);
  return row.data;
};
const nextCall = async continuation => {
  const row = await continuationRow(continuation);
  assert.notEqual(row.status, 'error', `${continuation.tool} result failed`);
  assert.equal(row.data.error, undefined, `${continuation.tool} returned an error payload`);
  return row.data;
};
const continuationRow = async continuation => {
  assert.ok(
    expectedTools.includes(continuation?.tool),
    'continuation has no runnable tool'
  );
  assert.ok(
    continuation.query && typeof continuation.query === 'object',
    'continuation has no query'
  );
  const response = await invoke(continuation.tool, { queries: [continuation.query] });
  const row = response.structuredContent?.results?.[0];
  assert.ok(row?.data, `${continuation.tool} has no result data`);
  if (response.isError) assert.equal(row.status, 'error');
  return row;
};
const executeCliTool = (name, queries) => {
  const startedAt = performance.now();
  const child = spawnSync(
    values.node,
    [path.resolve(values.cli), name, JSON.stringify({ queries: withTrace(queries) })],
    { encoding: 'utf8', timeout: 120_000, maxBuffer: 8 * 1024 * 1024, cwd: acceptanceCwd, env: acceptanceEnv }
  );
  const durationMs = Number((performance.now() - startedAt).toFixed(2));
  cliCalls.push({ name, durationMs, status: child.status });
  if (child.error) throw child.error;
  assert.ok(
    child.status === 0 || child.status === 6,
    `${name}: CLI exited ${child.status}: ${child.stderr.trim()}`
  );
  assert.ok(child.stdout.trim(), `${name}: CLI returned no JSON`);
  return JSON.parse(child.stdout);
};
const pages = async (first, nextKey, collect) => {
  let rows = [...collect(first)];
  let current = first;
  let count = 1;
  let restarts = 0;
  while (current.next?.[nextKey]) {
    assert.ok(count++ < 100, 'continuation did not terminate');
    const row = await continuationRow(current.next[nextKey]);
    if (row.status === 'error') {
      assert.equal(row.data.errorCode, 'lsp.snapshot.changed');
      assert.ok(row.data.next?.restart, 'changed snapshot has no restart continuation');
      assert.ok(restarts++ < 5, 'snapshot did not stabilize');
      const restarted = await continuationRow(row.data.next.restart);
      assert.notEqual(restarted.status, 'error', 'snapshot restart failed');
      current = restarted.data;
      rows = [...collect(current)];
      count = 1;
      continue;
    }
    current = row.data;
    rows.push(...collect(current));
  }
  return { rows, count };
};
const differingFields = (left, right, prefix = '', fields = []) => {
  if (fields.length >= 40) return fields;
  if (Object.is(left, right)) return fields;
  if (!left || !right || typeof left !== 'object' || typeof right !== 'object') {
    fields.push(prefix || '$');
    return fields;
  }
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of [...keys].sort()) differingFields(left[key], right[key], `${prefix}/${key}`, fields);
  return fields;
};

let pid;
try {
  await client.connect(transport);
  pid = transport.pid;
  transport.stderr?.on('data', chunk => {
    receipt.stderrBytes += chunk.length;
    receipt.stderrTail = `${receipt.stderrTail}${chunk.toString('utf8')}`.slice(-65_536);
  });
  const list = await client.listTools();
  receipt.catalog = list.tools;
  expectedTools = list.tools.map(tool => tool.name);
  receipt.catalogBytes = Buffer.byteLength(JSON.stringify(receipt.catalog));
  await check('initialize and list every available canonical direct tool', () =>
    assert.deepEqual(
      expectedTools.filter(name => name !== TOOL_NAMES.CLASIFY).sort(),
      canonicalTools.filter(name => name !== TOOL_NAMES.CLASIFY && !CLI_ONLY_TOOLS.has(name)).sort()
    )
  );
  await check('MCP tool catalog stays below the production transport budget', () =>
    assert.ok(
      receipt.catalogBytes <= 200_000,
      `serialized MCP catalog is ${receipt.catalogBytes} bytes`
    )
  );
  await check('MCP tool catalog omits output schemas', () =>
    assert.ok(
      list.tools.every(tool => !Object.hasOwn(tool, 'outputSchema')),
      'tools/list exposed an outputSchema'
    )
  );
  if (expectedTools.includes(TOOL_NAMES.CLASIFY)) {
    await check('clasify rejects caller model selection before provider access', async () => {
      const response = await invoke(TOOL_NAMES.CLASIFY, {
        id: 'caller-model-rejected',
        reasoning: 'Verify that provider model selection remains runtime-owned.',
        resources: [{ id: 'evidence', context: { value: 'Supplied evidence.' } }],
        questions: [{ id: 'bounded',
          type: 'noul',
          instructions: 'Is evidence supplied?',
        }],
        model: 'caller-model-is-forbidden',
      });
      const row = response.structuredContent?.results?.[0];
      assert.ok(response.isError || row?.status === 'error');
      assert.equal(row?.data?.usage, undefined);
      assert.match(JSON.stringify(response), /unknown field 'model'/i);
    });
  }
  await check('CLI catalog and MCP input schemas match the canonical contracts', () => {
    const definitions = new Map(getDirectToolDefinitionsWithAddons({
    }).map(definition => [definition.name, definition]));
    for (const tool of list.tools) {
      const cli = JSON.parse(
        execFileSync(
          values.node,
          [path.resolve(values.cli), 'scheme', tool.name],
          { encoding: 'utf8', timeout: 10_000, cwd: acceptanceCwd, env: acceptanceEnv }
        )
      );
      assert.equal(cli.name, tool.name);
      assert.equal(cli.availability.enabled, true);
      assert.deepEqual(cli.inputSchema, tool.inputSchema, `${tool.name} CLI and MCP input schemas differ`);
      assert.equal(Object.hasOwn(cli, 'querySchema'), false);
      assert.equal(Object.hasOwn(cli, 'outputSchema'), false);
      const definition = definitions.get(tool.name);
      assert.ok(definition, `${tool.name} has no canonical contract`);
      assert.deepEqual(tool.inputSchema, z.toJSONSchema(definition.inputSchema, {
        target: 'draft-2020-12',
        io: 'input',
      }), `${tool.name} MCP input schema differs from the canonical contract`);
    }
  });
  await check(
    'local file read has matching copy-safe text and structured content',
    async () => {
      const file = path.join(fixture, 'math.ts');
      const data = await call('localFetch', {
        path: file,
        minify: 'none',
      });
      assert.equal(data.content, await readFile(file, 'utf8'));
      assert.ok(
        receipt.calls.at(-1).response.content.some(block =>
          block.type === 'text'
          && block.text.includes('content (source lines):')
          && block.text.includes('1:// Arithmetic fixture.')
          && block.text.includes('2:export function add(left: number, right: number) { return left + right; }')
        )
      );
    }
  );
  await check('native MCP preserves ordered bulk query results', async () => {
    const response = await invoke('localFetch', {
      queries: [
        {
          reasoning: 'Read the first bulk fixture through native MCP.',
          debug: false,
          path: path.join(fixture, 'math.ts'),
          startLine: 1,
          endLine: 1,
        },
        {
          reasoning: 'Read the second bulk fixture through native MCP.',
          debug: false,
          path: path.join(fixture, 'entry.ts'),
          startLine: 1,
          endLine: 1,
        },
      ],
    });
    assert.equal(response.isError, false);
    const rows = response.structuredContent?.results;
    assert.equal(rows?.length, 2);
    assert.deepEqual(rows.map(row => row.index), [0, 1]);
    assert.ok(rows[0].data.content.includes('Arithmetic fixture'));
    assert.ok(rows[1].data.content.includes('import'));
  });
  await check('localFetch line and byte chunks preserve selected views through real MCP', async () => {
    const directory = await mkdtemp(path.join(path.resolve('.octocode/tmp'), 'fetch-chunks-'));
    const file = path.join(directory, 'source.txt');
    const source = 'skip\r\nneedle 🌍\r\n\r\nneedle café\nlast\n';
    try {
      await writeFile(file, source);
      for (const chunkType of ['lines', 'bytes']) {
        for (const matched of [false, true]) {
          let page = await call('localFetch', {
            path: file, chunkType, chunkSize: chunkType === 'lines' ? 1 : 3,
            ...(matched ? { matchString: 'needle', contextLines: 0, minify: 'standard' } : {}),
          });
          let content = '';
          let count = 0;
          for (;;) {
            assert.ok(++count < 100);
            assert.equal(page.totalLines, 5);
            assert.equal(page.sourceBytes, Buffer.byteLength(source));
            assert.equal(page.returnedBytes, Buffer.byteLength(page.content));
            if (matched) assert.equal(page.minifyFallback.reason, 'match-evidence');
            content += page.content;
            if (!page.next?.continue) break;
            assert.equal(page.next.continue.tool, 'localFetch');
            page = await nextCall(page.next.continue);
          }
          assert.equal(
            content,
            matched
              ? 'needle 🌍\r\n... [line 3 omitted] ...\nneedle café\n'
              : source
          );
        }
      }
      const invalid = await invoke('localFetch', {
        queries: [{
          reasoning: 'Verify retired localFetch charLength input is rejected.',
          debug: false,
          path: file,
          charLength: 3,
        }],
      });
      assert.equal(invalid.isError, true);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
  for (const [regex, searchText] of [
    ['literal', 'add'],
    ['rust', '\\badd\\b'],
    ['pcre2', '(?<!\\w)add(?!\\w)'],
  ]) {
    await check(`local text search returns the observed anchor (${regex})`, async () => {
      const data = await call('localSearch', {
        path: path.join(fixture, 'math.ts'),
        searchText,
        regex,
        wholeWord: true,
        resultView: 'content',
      });
      assert.equal(data.stats.totalOccurrences, 1);
      assert.equal(data.stats.capped, false);
      assert.equal(data.files[0].matches[0].line, 2);
      assert.ok(data.files[0].matches[0].value.includes('function add'));
    });
  }
  await check('empty local search returns one concise recovery hint', async () => {
    const data = await call('localSearch', {
      path: fixture,
      searchText: 'octocode-definitely-absent-token',
      regex: 'literal',
      resultView: 'files',
    });
    assert.equal(data.stats.totalOccurrences, 0);
    assert.equal(data.hints?.length, 1);
    assert.match(data.hints[0], /try|shorter|case|regex/i);
  });
  await check('local file discovery positive', async () => {
    const data = await call('structureSearch', {
      operation: 'files',
      path: fixture,
      extensions: ['ts'],
      pageSize: 50,
    });
    assert.ok(data.files.some(file => file.path.endsWith('math.ts')));
  });
  await check('AST symbols identify the exported arithmetic declaration', async () => {
    const data = await call('astSearch', {
      operation: 'symbols',
      path: path.join(fixture, 'math.ts'),
      name: 'add',
      kinds: ['function'],
    });
    assert.equal(data.operation, 'symbols');
    assert.equal(data.totalDeclarations, 1);
    assert.equal(data.declarations[0].name, 'add');
    assert.equal(data.declarations[0].kind, 'function');
    assert.equal(data.declarations[0].line, 2);
    assert.equal(data.declarations[0].exported, true);
  });
  await check('astRewrite is CLI-only: MCP rejects it, the CLI previews and applies on an isolated fixture', async () => {
    assert.ok(!expectedTools.includes('astRewrite'), 'MCP must not list astRewrite');
    // An unlisted tool is a protocol error (-32602) or an error result, never a rewrite.
    const rejected = await client
      .callTool({ name: 'astRewrite', arguments: { queries: [{ reasoning: 'Verify MCP never rewrites.', path: fixture, langType: 'typescript', ruleKind: 'pattern', pattern: 'oldCall($A)', rewrite: 'newCall($A)' }] } })
      .then(result => result.isError === true, error => /not found|not available/i.test(String(error?.message)));
    assert.ok(rejected, 'MCP must reject astRewrite');
    const directory = await mkdtemp(path.join(path.resolve('.octocode/tmp'), 'cli-rewrite-'));
    const file = path.join(directory, 'source.ts');
    try {
      await writeFile(file, 'oldCall(1);\noldCall(2);\n');
      const rule = { reasoning: 'Verify CLI rewrite.', path: directory, langType: 'typescript', ruleKind: 'pattern', pattern: 'oldCall($A)', rewrite: 'newCall($A)', pageSize: 10 };
      const preview = executeCliTool('astRewrite', [rule]).results[0].data;
      assert.equal(preview.mode, 'preview');
      assert.equal(preview.totalMatches, 2);
      const applied = executeCliTool('astRewrite', [{
        ...rule,
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes: Object.fromEntries(
          preview.files.map(item => [path.join(directory, item.path), item.beforeHash])
        ),
      }]).results[0].data;
      assert.equal(applied.mode, 'apply');
      assert.equal(applied.transaction.committed, true);
      assert.equal(await readFile(file, 'utf8'), 'newCall(1);\nnewCall(2);\n');
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
  if (!values.quick) {
    await check(
      'outer text pagination isolates structured rows and reconstructs every character',
      async () => {
        const args = {
          queries: [{
            reasoning: 'Exercise localFetch response pagination through built stdio acceptance.',
            debug: false,
            path: path.join(fixture, 'math.ts'),
            minify: 'none',
          }],
        };
        const full = await invoke('localFetch', args);
        let current = await invoke('localFetch', {
          ...args,
          responseCharLength: 150,
        });
        let text = '';
        let count = 0;
        assert.ok(full.structuredContent.results.length > 0);
        while (true) {
          assert.ok(count++ < 30);
          assert.deepEqual(
            current.structuredContent.results,
            []
          );
          // structuredContent-only hosts must still see the page window.
          assert.equal(
            current.structuredContent.responseWindow,
            current.content
              .filter(block => block.type === 'text')
              .map(block => block.text)
              .join('')
          );
          text += current.content
            .filter(block => block.type === 'text')
            .map(block => block.text.replace(/^# Response page[^\n]*\n/, ''))
            .join('');
          const next = current.structuredContent.responsePagination?.next;
          if (!next) break;
          current = await invoke(next.tool, next.query);
        }
        assert.ok(count > 1);
        assert.equal(
          text,
          full.content
            .filter(block => block.type === 'text')
            .map(block => block.text)
            .join('')
        );
      }
    );
    await check(
      'file pagination executes continuations and preserves full inventory',
      async () => {
        const query = { operation: 'files', path: fixture, extensions: ['ts'] };
        const full = await call('structureSearch', { ...query, pageSize: 50 });
        const first = await call('structureSearch', { ...query, pageSize: 1 });
        const paged = await pages(first, 'nextPage', data =>
          data.files.map(file => file.path)
        );
        assert.ok(paged.count > 1);
        assert.deepEqual(
          paged.rows.sort(),
          full.files.map(file => file.path).sort()
        );
      }
    );
    await check('changed whole-response snapshots restart through MCP', async () => {
      const parent = path.resolve('.octocode/tmp');
      await mkdir(parent, { recursive: true });
      const directory = await mkdtemp(path.join(parent, 'mcp-snapshot-'));
      const file = path.join(directory, 'source.ts');
      try {
        await writeFile(file, 'export const value = 1;\n');
        const first = await invoke('localFetch', {
          queries: [{
            reasoning: 'Exercise stale localFetch response pagination through built stdio acceptance.',
            debug: false,
            path: file,
            minify: 'none',
          }],
          responseCharLength: 100,
        });
        const before = first.structuredContent.responsePagination;
        assert.ok(before.next);
        await writeFile(file, 'export const value = 200;\n');
        const changed = await invoke(before.next.tool, before.next.query);
        const restart = changed.structuredContent.responsePagination;
        assert.equal(restart.restart, true);
        assert.equal(restart.changed, true);
        assert.equal(restart.expectedSnapshot, before.snapshot);
        assert.equal(restart.charLength, 0);
        assert.equal(restart.next.query.responseCharOffset, 0);
        const restarted = await invoke(restart.next.tool, restart.next.query);
        assert.equal(restarted.structuredContent.responsePagination.charOffset, 0);
        assert.notEqual(restarted.structuredContent.responsePagination.snapshot, before.snapshot);
      } finally {
        await rm(directory, { recursive: true, force: true });
      }
    });
    await check('structural captures positive', async () => {
      const data = await call('astSearch', {
        operation: 'match',
        path: fixture,
        pattern: 'add($$$ARGS)',
        langType: 'typescript',
        captureText: true,
      });
      assert.ok(JSON.stringify(data).includes('add(value, value)'));
    });
    await check('file graph dependency positive', async () => {
      const data = await call('astTopology', {
        analysis: 'dependencies',
        path: fixture,
        file: 'entry.ts',
        depth: 2,
        excludeDir: ['coverage', 'removed', 'rust'],
      });
      assert.ok(JSON.stringify(data).includes('math.ts'));
    });
    await check(
      'graph diagnostic continuation union preserves the complete inventory',
      async () => {
        const query = {
          analysis: 'dependencies',
          path: `${fixture}-diagnostics`,
          file: 'entry.ts',
          depth: 3,
          pageSize: 50,
          excludeDir: [],
        };
        const full = await call('astTopology', {
          ...query,
          diagnosticPageSize: 100,
        });
        const first = await call('astTopology', {
          ...query,
          diagnosticPageSize: 2,
        });
        const paged = await pages(
          first,
          'nextDiagnostics',
          data => data.coverage.diagnostics ?? []
        );
        assert.ok(paged.count > 1);
        assert.deepEqual(paged.rows, full.coverage.diagnostics);
      }
    );
    await check('LSP definition identifies the declaration', async () => {
      const data = await call('lspSearch', {
        uri: path.join(fixture, 'entry.ts'),
        workspaceRoot: fixture,
        operation: 'definition',
        symbolName: 'add',
        lineHint: 4,
      });
      assert.equal(data.payload.kind, 'definition');
      assert.ok(
        data.payload.locations.some(
          location =>
            location.path.endsWith('math.ts') &&
            location.displayRange.startLine === 2
        )
      );
    });
    await check(
      'LSP references execute snapshot continuations without loss',
      async () => {
        const query = {
          uri: path.join(fixture, 'math.ts'),
          workspaceRoot: fixture,
          operation: 'references',
          symbolName: 'add',
          lineHint: 2,
        };
        const first = await call('lspSearch', { ...query, pageSize: 1 });
        const paged = await pages(
          first,
          'nextPage',
          data => data.payload.locations
        );
        const full = await call('lspSearch', { ...query, pageSize: 100 });
        assert.ok(paged.count > 1);
        assert.deepEqual(paged.rows, full.payload.locations);
      }
    );
    for (const name of expectedTools)
      await check(
        `${name} rejects malformed arguments without killing stdio`,
        async () => {
          const result = await invoke(name, { queries: 'invalid' });
          assert.equal(result.isError, true);
          receipt.guidance.validationErrors += 1;
          const message = result.content
            .filter(block => block.type === 'text')
            .map(block => block.text)
            .join('\n');
          assert.match(message, /queries|array|invalid/i);
          assert.ok(message.length > 0 && message.length <= 2_000, `${name}: unusable validation message`);
          assert.equal((await client.listTools()).tools.length, expectedTools.length);
        }
      );
    for (const removedName of ['localAnalyzeGraph', 'lspGetSemantics', 'octocode_nonexistent_tool']) await check(
      `${removedName} returns an error and server remains responsive`,
      async () => {
        let rejected = false;
        try {
          const result = await client.callTool({
            name: removedName,
            arguments: {},
          });
          rejected = result.isError === true;
        } catch {
          rejected = true;
        }
        assert.ok(rejected);
        assert.equal((await client.listTools()).tools.length, expectedTools.length);
      }
    );
  }
  if (values.live && !values.quick) {
    if (expectedTools.includes(TOOL_NAMES.CLASIFY)) {
      await check('clasify executes Noul, Choice, and Score through the live provider', async () => {
        const response = await invoke(TOOL_NAMES.CLASIFY, { queries: [{
          id: 'live-primitives',
          reasoning: 'Verify every semantic primitive through the built MCP surface.',
          resources: [{
            id: 'fixture',
            context: { value: 'The fixture explicitly states that alpha is enabled.' },
          }],
          questions: [
            {
              id: 'noul',
              type: 'noul',
              instructions: 'Does the fixture state that alpha is enabled?',
            },
            {
              id: 'choice',
              type: 'choice',
              instructions: 'Which state does the fixture assign to alpha?',
              criteria: { enabled: null, disabled: null },
            },
            {
              id: 'score',
              type: 'score',
              instructions: 'How explicit is the fixture about alpha being enabled?',
              criteria: ['not stated', 'implied', 'explicitly stated'],
            },
          ],
        }] });
        assert.equal(response.isError, false);
        const resource = response.structuredContent?.queries?.[0]?.resources?.[0];
        assert.equal(resource?.coverage, 'complete');
        const answers = resource?.pages?.[0]?.answers;
        assert.ok(typeof answers?.noul?.noul === 'number');
        assert.ok(typeof answers?.choice?.choice === 'string');
        assert.ok(typeof answers?.score?.score === 'number');
      });
    }
    const repo = { owner: 'octocat', repo: 'Hello-World' };
    const sha = '7fd1a60b01f91b314f59955a4e4d4e80d8edf11d';
    await check('GitHub tree preserves paths and the requested revision', async () => {
      const data = await call('ghStructure', { ...repo, branch: sha, pageSize: 1 });
      assert.equal(data.resolvedBranch, sha);
      assert.ok(data.structure.some(directory => directory.files.includes('README')));
    });
    await check('GitHub full file reads return content without a checkout', async () => {
      const data = await call('ghGetFileContent', { ...repo, branch: sha, path: 'README', fullContent: true });
      assert.equal(data.files[0].content, 'Hello World!\n');
      assert.equal(data.files[0].localPath, undefined);
      assert.equal(data.files[0].repoRoot, undefined);
      assert.equal(data.directories, undefined);
    });
    await check('GitHub file fetch rejects directory paths and removed directory mode', async () => {
      const directory = await invoke('ghGetFileContent', { queries: [{ ...repo, branch: sha, path: '' }] });
      assert.equal(directory.isError, true);
      assert.match(
        JSON.stringify(directory.structuredContent ?? directory.content ?? directory),
        /non-blank path|directory/i
      );
      let rejected = false;
      try {
        const result = await invoke('ghGetFileContent', { queries: [{ ...repo, path: '', type: 'directory' }] });
        rejected = result.isError === true;
      } catch { rejected = true; }
      assert.ok(rejected);
    });
    await check('GitHub repository search positive', async () => {
      const data = await call('ghSearchRepo', {
        owner: 'octocat',
        keywords: ['Hello-World'],
        pageSize: 1,
      });
      assert.ok(JSON.stringify(data).includes('Hello-World'));
    });
    await check('GitHub code search positive', async () => {
      const data = await call('ghSearchCode', {
        owner: 'jonschlinkert',
        repo: 'is-number',
        filename: 'index.js',
        keywords: ['module.exports'],
        pageSize: 1,
      });
      assert.ok(data.files.length > 0);
    });
    await check(
      'GitHub exact file byte continuations preserve all bytes',
      async () => {
        const query = { ...repo, branch: sha, path: 'README', minify: 'none' };
        const full = await call('ghGetFileContent', query);
        let current = await call('ghGetFileContent', {
          ...query,
          chunkType: 'bytes', chunkSize: 5,
        });
        let content = current.files[0].content;
        let count = 1;
        while (current.files[0].next?.continue) {
          assert.ok(count++ < 20);
          current = await nextCall(current.files[0].next.continue);
          content += current.files[0].content;
        }
        assert.ok(count > 1);
        assert.equal(content, full.files[0].content);
      }
    );
    await check('GitHub/local search-to-match fetch parity in both chunk units', async () => {
      const repo = { owner: 'jonschlinkert', repo: 'is-number' };
      const found = await call('ghSearchCode', { ...repo, filename: 'index.js', keywords: ['module.exports'], pageSize: 1 });
      assert.ok(found.files?.length > 0);
      const remotePath = found.files[0].path;
      const full = await call('ghGetFileContent', { ...repo, path: remotePath, fullContent: true });
      const source = full.files[0].content;
      assert.ok(source.includes('module.exports'));
      const parent = path.resolve('.octocode/tmp');
      const directory = await mkdtemp(path.join(parent, 'remote-local-fetch-'));
      const file = path.join(directory, 'index.js');
      try {
        await writeFile(file, source);
        const localFound = await call('localSearch', { path: directory, searchText: 'module.exports', regex: 'literal' });
        assert.ok(localFound.files?.length > 0);
        for (const chunkType of ['lines', 'bytes']) {
          const selector = { matchString: 'module.exports', contextLines: 2, minify: 'standard', chunkType, chunkSize: chunkType === 'lines' ? 1 : 7 };
          const contents = [];
          const anchors = [];
          for (const remote of [false, true]) {
            const tool = remote ? 'ghGetFileContent' : 'localFetch';
            const query = remote ? { ...repo, path: remotePath, ...(full.files[0].commitSha ? { branch: full.files[0].commitSha } : {}), ...selector } : { path: file, ...selector };
            let result = await call(tool, query);
            let joined = '';
            let count = 0;
            const matched = new Set();
            while (true) {
              assert.ok(count++ < 100);
              const page = remote ? result.files[0] : result;
              assert.equal(page.sourceBytes, Buffer.byteLength(source));
              assert.equal(page.returnedBytes, Buffer.byteLength(page.content));
              assert.equal(page.minifyFallback.reason, 'match-evidence');
              joined += page.content;
              for (const line of page.matchedLines ?? []) matched.add(line);
              if (!page.next?.continue) { assert.equal(page.pagination.hasMore, false); break; }
              assert.equal(page.next.continue.query.matchString, selector.matchString);
              result = await nextCall(page.next.continue);
            }
            assert.ok(count > 1);
            contents.push(joined);
            anchors.push([...matched]);
          }
          assert.equal(contents[0], contents[1]);
          assert.deepEqual(anchors[0], anchors[1]);
          assert.ok(anchors[0].length > 0);
        }
      } finally { await rm(directory, { recursive: true, force: true }); }
    });
    await check('GitHub commit history search positive', async () => {
      const data = await call('ghSearchHistory', {
        ...repo,
        operation: 'commit',
        pageSize: 1,
      });
      assert.ok(JSON.stringify(data).includes(sha));
      assert.ok(data.next?.nextPage);
      const next = await nextCall(data.next.nextPage);
      assert.equal(next.pagination.currentPage, 2);
    });
    await check('GitHub exact commit positive', async () => {
      const data = await call('ghGetHistoryItem', {
        ...repo,
        operation: 'commit',
        ref: sha,
        includeDiff: true,
      });
      assert.ok(JSON.stringify(data).includes(sha));
    });
    await check('npm exact metadata positive', async () => {
      const data = await call('artifactSearch', { type: 'npm', packageName: 'is-number' });
      assert.ok(JSON.stringify(data).includes('7.0.0'));
    });
    await check('npm discovery continuation is executable', async () => {
      const data = await call('artifactSearch', {
        type: 'npm',
        keywords: ['is-number'],
        pageSize: 1,
      });
      assert.ok(data.next?.nextPage);
      await nextCall(data.next.nextPage);
    });
    await check('CLI clone pinned revision positive', async () => {
      const data = executeCliTool('ghCloneRepo', [{ ...repo, branch: sha, reasoning: 'Verify CLI-only clone.' }]).results[0].data;
      assert.ok(data.location.localPath);
      assert.equal(data.location.commitSha, sha);
      assert.equal(
        await readFile(path.join(data.location.localPath, 'README'), 'utf8'),
        'Hello World!\n'
      );
    });
    if (expectedTools.includes(TOOL_NAMES.CLASIFY)) {
      await check('clasify captures every allowed resource tool and completes continuations', async () => {
        const resourceTools = new Set();
        const visit = value => {
          if (!value || typeof value !== 'object') return;
          for (const name of value.properties?.tool?.enum ?? []) resourceTools.add(name);
          Object.values(value).forEach(visit);
        };
        visit(list.tools.find(tool => tool.name === TOOL_NAMES.CLASIFY).inputSchema);
        assert.ok(resourceTools.size > 0);
        const resources = [...resourceTools].map(tool => {
          const sample = receipt.calls.find(call =>
            call.name === tool && call.response.isError === false
            && call.response.structuredContent?.results?.[0]?.data
            && !['empty', 'error'].includes(call.response.structuredContent.results[0].status)
          );
          assert.ok(sample, `${tool}: no successful resource fixture`);
          return { id: tool, context: { tool, query: sample.arguments.queries[0] } };
        });
        let query = {
          resources,
          questions: [{ id: 'evidence', type: 'noul', instructions: 'Does this evidence identify a named file, repository, package, or code symbol?' }],
        };
        const completed = new Set();
        let calls = 0;
        while (query) {
          assert.ok(calls++ < 20, 'clasify continuations did not terminate');
          const response = await invoke(TOOL_NAMES.CLASIFY, query);
          assert.equal(response.isError, false);
          const matrix = response.structuredContent?.queries?.[0];
          assert.ok(matrix?.resources?.length);
          assert.deepEqual(matrix.resources.map(resource => resource.resourceId), query.resources.map(resource => resource.id));
          for (const resource of matrix.resources) {
            assert.ok(['complete', 'partial'].includes(resource.coverage), JSON.stringify(resource));
            assert.ok(resource.pages.length > 0);
            for (const page of resource.pages) {
              assert.equal(page.error, undefined);
              const answer = page.answers?.evidence?.noul;
              assert.ok(typeof answer === 'number' && answer >= 0 && answer <= 1, JSON.stringify(page));
              assert.equal(page.content, undefined);
              assert.equal(page.body, undefined);
            }
            if (resource.coverage === 'complete') completed.add(resource.resourceId);
            else assert.ok(matrix.next?.clasify?.resources.some(next => next.id === resource.resourceId));
          }
          query = matrix.next?.clasify;
        }
        assert.deepEqual([...completed].sort(), [...resourceTools].sort());
      });
    }
  }
  if (values['cli-mcp-parity']) {
    await check('deterministic same-query CLI and real MCP structured result parity', async () => {
      const parity = [];
      receipt.cliMcpParity = parity;
      const cacheVolatileTools = new Set([
        'ghSearchRepo', 'ghSearchCode', 'ghStructure', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'artifactSearch',
      ]);
      const liveOnlyTools = cacheVolatileTools;
      for (const name of expectedTools) {
        if (name === TOOL_NAMES.CLASIFY) {
          const executionVerified = receipt.calls.some(call =>
            call.name === name
            && call.response.isError === false
            && call.response.structuredContent?.results?.[0]?.status !== 'error'
          );
          parity.push({
            name,
            status: 'not-applicable',
            comparison: 'exact-structured-results',
            executionVerified,
            reason: 'Independent probabilistic responses need not be identical; executionVerified records the separate live provider check.',
          });
          assert.ok(
            !values.live || executionVerified,
            'clasify: --live requires one successful provider-backed execution'
          );
          continue;
        }
        const sample = cliMcpParitySamples.get(name);
        const selected = sample?.selected ?? receipt.calls.find(call =>
          call.name === name
          && call.response.isError === false
          && call.response.structuredContent?.results?.[0]?.status !== 'error'
          && call.response.structuredContent?.results?.[0]?.status !== 'empty'
        );
        if (
          !selected
          && liveOnlyTools.has(name)
          && (!values.live || values.quick)
        ) {
          parity.push({
            name,
            status: 'not-run',
            reason: 'Successful provider-backed parity requires --live without --quick.',
          });
          continue;
        }
        assert.ok(selected, `${name}: no successful non-mutating MCP call to compare`);
        const cliResponse = sample?.cliResponse ?? executeCliTool(name, selected.arguments.queries);
        const cliResults = sample?.cliResults ?? cliResponse.results;
        const mcpResults = selected.response.structuredContent.results;
        const mcpBase = selected.response.structuredContent.base;
        const cliBase = cliResponse?.base;
        const differences = differingFields(mcpResults, cliResults);
        // Provider cache state depends on which cross-process arm reached the
        // provider first. For these provider tools it is receipt metadata,
        // not evidence or tool data; retain the raw pair and compare all other
        // fields explicitly. No local/AST/LSP result receives this exception.
        const cacheOnly = cacheVolatileTools.has(name) && differences.every(field => field === '/0/cache');
        const mcpComparable = cacheOnly ? mcpResults.map(({ cache, ...row }) => row) : mcpResults;
        const cliComparable = cacheOnly ? cliResults.map(({ cache, ...row }) => row) : cliResults;
        const contractDifferences = differingFields(mcpComparable, cliComparable)
          .filter(field => !field.endsWith('/cursor'));
        const opaqueCursorDifferences = differences.filter(field => field.endsWith('/cursor'));
        const baseEqual = mcpBase === cliBase;
        parity.push({
          name,
          arguments: selected.arguments,
          mcpResults,
          cliResults,
          mcpBase,
          cliBase,
          baseEqual,
          exact: differences.length === 0,
          rawDifferingFields: differences,
          comparison: cacheOnly || opaqueCursorDifferences.length
            ? 'evidence-data-and-executable-query'
            : 'exact-structured-results',
          cacheExclusionJustification: cacheOnly ? 'provider cache state is cross-process timing metadata; all evidence and data fields remain exact' : undefined,
          opaqueCursorExclusionJustification: opaqueCursorDifferences.length
            ? 'opaque cursors may differ across independent processes; the continuation tool and executable query remain exact'
            : undefined,
          differingFields: contractDifferences,
          passesContract: contractDifferences.length === 0 && baseEqual,
        });
      }
      receipt.cliMcpParitySummary = {
        compared: parity.filter(row => row.passesContract !== undefined).length,
        notApplicable: parity.filter(row => row.status === 'not-applicable').map(row => row.name),
        notRun: parity.filter(row => row.status === 'not-run').map(row => row.name),
      };
      const failures = parity.filter(row => !['not-run', 'not-applicable'].includes(row.status) && !row.passesContract);
      assert.deepEqual(failures.map(row => ({ name: row.name, baseEqual: row.baseEqual, differingFields: row.differingFields })), []);
    });
  }
  await check('stdio contains no parser or protocol errors', () =>
    assert.deepEqual(receipt.transportErrors, [])
  );
} finally {
  const start = Date.now();
  try {
    await client.close();
  } catch (error) {
    receipt.closeError = error.message;
  }
  receipt.shutdownMs = Date.now() - start;
  const summarizeLatencies = calls => Object.fromEntries(
    [...new Set(calls.map(call => call.name))].sort().map(name => {
      const durations = calls
        .filter(call => call.name === name)
        .map(call => call.durationMs)
        .sort((left, right) => left - right);
      const percentile = fraction =>
        durations[Math.min(durations.length - 1, Math.ceil(durations.length * fraction) - 1)];
      return [name, {
        count: durations.length,
        p50Ms: percentile(0.5),
        p95Ms: percentile(0.95),
        maxMs: durations.at(-1),
        totalMs: Number(durations.reduce((sum, duration) => sum + duration, 0).toFixed(2)),
      }];
    })
  );
  receipt.latencyByTool = summarizeLatencies(receipt.calls);
  receipt.cliLatencyByTool = summarizeLatencies(cliCalls);
  receipt.responseBytesByTool = Object.fromEntries(
    [...new Set(receipt.calls.map(call => call.name))].sort().map(name => {
      const sizes = receipt.calls
        .filter(call => call.name === name)
        .map(call => call.responseBytes)
        .sort((left, right) => left - right);
      const percentile = fraction =>
        sizes[Math.min(sizes.length - 1, Math.ceil(sizes.length * fraction) - 1)];
      return [name, {
        count: sizes.length,
        p50Bytes: percentile(0.5),
        p95Bytes: percentile(0.95),
        maxBytes: sizes.at(-1),
      }];
    })
  );
  receipt.totalMs = Number((performance.now() - suiteStartedAt).toFixed(2));
  await check('child shuts down and releases its PID', () => {
    assert.ok(pid);
    assert.throws(() => process.kill(pid, 0));
    assert.ok(receipt.shutdownMs < 7_000);
  });
  await mkdir(path.dirname(path.resolve(values.receipt)), { recursive: true });
  await writeFile(values.receipt, JSON.stringify(receipt, null, 2));
  if (!values.fixture) {
    await rm(fixture, { recursive: true, force: true });
    await rm(`${fixture}-diagnostics`, { recursive: true, force: true });
  }
}
const failures = receipt.checks.filter(check => check.status === 'failed');
console.log(
  JSON.stringify({
    passed: receipt.checks.length - failures.length,
    failures,
    calledTools: [...new Set(receipt.calls.map(call => call.name))],
    latencyByTool: receipt.latencyByTool,
    cliLatencyByTool: receipt.cliLatencyByTool,
    responseBytesByTool: receipt.responseBytesByTool,
    guidance: receipt.guidance,
    totalMs: receipt.totalMs,
    receipt: values.receipt,
    shutdownMs: receipt.shutdownMs,
  })
);
if (failures.length) process.exitCode = 1;
