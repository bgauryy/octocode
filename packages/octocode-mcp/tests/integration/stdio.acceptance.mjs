/** Real built-server acceptance. Run after building CLI + MCP; no mocks or installs. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
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
  ENABLE_CLONE: 'true',
  ENABLE_AST_REWRITE_APPLY: 'true',
  OCTOCODE_STORAGE_MODE: 'persistent',
};
const { DIRECT_TOOL_DISCOVERY_DEFINITIONS } = await import('@octocodeai/octocode-core/schema');
const expectedTools = DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name);
const receipt = {
  server: path.resolve(values.server),
  node: values.node,
  fixture,
  checks: [],
  calls: [],
  transportErrors: [],
  stderrBytes: 0,
};
const cliMcpParitySamples = new Map();
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
const invoke = async (name, args) => {
  const response = await client.callTool({ name, arguments: args });
  receipt.calls.push({ name, arguments: args, response });
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
        }
        if (key === 'next' && !recovery) {
          for (const [nextKey, call] of Object.entries(child)) {
            assert.ok(!['fetch', 'getLines', 'readSite', 'viewTree', 'viewStructure', 'cloneRepo', 'searchRepositoryCode', 'lspDefinition', 'lspReferences'].includes(nextKey), `${name}: unsolicited ${nextKey}`);
            assert.equal(call.why, undefined, `${name}: success continuation prose`);
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
  assert.ok(
    expectedTools.includes(continuation?.tool),
    'continuation has no runnable tool'
  );
  assert.ok(
    continuation.query && typeof continuation.query === 'object',
    'continuation has no query'
  );
  return call(continuation.tool, continuation.query);
};
const executeCliTool = (name, queries) => JSON.parse(execFileSync(
  values.node,
  [path.resolve(values.cli), 'tools', name, '--queries', JSON.stringify(queries), '--compact'],
  { encoding: 'utf8', timeout: 120_000, maxBuffer: 8 * 1024 * 1024, cwd: acceptanceCwd, env: acceptanceEnv }
));
const pages = async (first, nextKey, collect) => {
  const rows = [...collect(first)];
  let current = first;
  let count = 1;
  while (current.next?.[nextKey]) {
    assert.ok(count++ < 100, 'continuation did not terminate');
    current = await nextCall(current.next[nextKey]);
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
  });
  const list = await client.listTools();
  receipt.catalog = list.tools.map(tool => ({
    name: tool.name,
    inputSchema: tool.inputSchema,
    outputSchema: tool.outputSchema ?? null,
  }));
  await check('initialize and list every canonical direct tool', () =>
    assert.deepEqual(
      list.tools.map(t => t.name).sort(),
      [...expectedTools].sort()
    )
  );
  await check('CLI and MCP input schema parity for every tool', () => {
    for (const tool of list.tools) {
      const cli = JSON.parse(
        execFileSync(
          values.node,
          [path.resolve(values.cli), 'tools', tool.name, '--scheme', '--json'],
          { encoding: 'utf8', timeout: 10_000, cwd: acceptanceCwd, env: acceptanceEnv }
        )
      );
      assert.deepEqual(
        tool.inputSchema,
        cli.inputSchema,
        `${tool.name} input schemas differ`
      );
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
          && block.text.includes('1: // Arithmetic fixture.')
          && block.text.includes('2: export function add(left: number, right: number) { return left + right; }')
        )
      );
    }
  );
  await check('localFetch line and byte chunks preserve selected views through real MCP', async () => {
    const directory = await mkdtemp(path.join(path.resolve('.octocode/tmp'), 'fetch-chunks-'));
    const file = path.join(directory, 'source.txt');
    const source = 'skip\r\nneedle 🌍\r\n\r\nneedle café\nlast\n';
    try {
      await writeFile(file, source);
      for (const chunkType of ['lines', 'bytes']) {
        for (const matched of [false, true]) {
          let page = await call('localFetch', {
            path: file, chunkType, limit: chunkType === 'lines' ? 1 : 3,
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
          assert.equal(content, matched ? 'needle 🌍\r\nneedle café\n' : source);
        }
      }
      const invalid = await invoke('localFetch', { queries: [{ path: file, charLength: 3 }] });
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
  await check('local file discovery positive', async () => {
    const data = await call('astSearch', {
      operation: 'files',
      path: fixture,
      extensions: ['ts'],
      pageSize: 50,
    });
    assert.ok(data.files.some(file => file.path.endsWith('math.ts')));
  });
  await check('astRewrite previews and guarded apply mutate only an isolated fixture', async () => {
    const directory = await mkdtemp(path.join(path.resolve('.octocode/tmp'), 'mcp-rewrite-'));
    const file = path.join(directory, 'source.ts');
    try {
      await writeFile(file, 'oldCall(1);\noldCall(2);\n');
      const preview = await call('astRewrite', {
        path: directory, langType: 'typescript', ruleKind: 'pattern',
        pattern: 'oldCall($A)', rewrite: 'newCall($A)', pageSize: 10,
      });
      assert.equal(preview.mode, 'preview');
      assert.equal(preview.totalMatches, 2);
      if (values['cli-mcp-parity']) {
        const selected = receipt.calls.at(-1);
        const cliResponse = executeCliTool('astRewrite', selected.arguments.queries);
        cliMcpParitySamples.set('astRewrite', { selected, cliResponse, cliResults: cliResponse.results });
      }
      const applied = await call('astRewrite', {
        path: directory, langType: 'typescript', ruleKind: 'pattern',
        pattern: 'oldCall($A)', rewrite: 'newCall($A)', apply: true,
        pageSize: 10,
        snapshot: preview.snapshot,
        expectedHashes: Object.fromEntries(preview.files.map(item => [item.absolutePath, item.beforeHash])),
      });
      assert.equal(applied.mode, 'apply');
      assert.equal(applied.transaction.committed, true);
      assert.equal(await readFile(file, 'utf8'), 'newCall(1);\nnewCall(2);\n');
      assert.equal(await readFile(path.join(fixture, 'math.ts'), 'utf8'), '// Arithmetic fixture.\nexport function add(left: number, right: number) { return left + right; }\n');
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
  if (!values.quick) {
    await check(
      'outer text pagination preserves structured data and reconstructs every character',
      async () => {
        const args = {
          queries: [{ path: path.join(fixture, 'math.ts'), minify: 'none' }],
        };
        const full = await invoke('localFetch', args);
        let current = await invoke('localFetch', {
          ...args,
          responseCharLength: 150,
        });
        let text = '';
        let count = 0;
        while (true) {
          assert.ok(count++ < 30);
          assert.deepEqual(
            current.structuredContent.results,
            full.structuredContent.results
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
        const full = await call('astSearch', { ...query, pageSize: 50 });
        const first = await call('astSearch', { ...query, pageSize: 1 });
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
          queries: [{ path: file, minify: 'none' }],
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
      const data = await call('astSearch', {
        operation: 'topology',
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
          operation: 'topology',
          analysis: 'dependencies',
          path: `${fixture}-diagnostics`,
          file: 'entry.ts',
          depth: 3,
          pageSize: 50,
          excludeDir: [],
        };
        const full = await call('astSearch', {
          ...query,
          diagnosticPageSize: 100,
        });
        const first = await call('astSearch', {
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
            location.uri.endsWith('/math.ts') &&
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
          const result = await client.callTool({
            name,
            arguments: { queries: 'invalid' },
          });
          assert.equal(result.isError, true);
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
    const repo = { owner: 'octocat', repo: 'Hello-World' };
    const sha = '7fd1a60b01f91b314f59955a4e4d4e80d8edf11d';
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
      const data = await call('ghSearch', {
        operation: 'repositories',
        owner: 'octocat',
        keywords: ['Hello-World'],
        pageSize: 1,
      });
      assert.ok(JSON.stringify(data).includes('Hello-World'));
    });
    await check('GitHub code search positive', async () => {
      const data = await call('ghSearch', {
        operation: 'code',
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
          chunkType: 'bytes', limit: 5,
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
      const found = await call('ghSearch', { ...repo, operation: 'code', filename: 'index.js', keywords: ['module.exports'], pageSize: 1 });
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
          const selector = { matchString: 'module.exports', contextLines: 2, minify: 'standard', chunkType, limit: chunkType === 'lines' ? 1 : 7 };
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
        operation: 'commits',
        pageSize: 1,
      });
      assert.ok(JSON.stringify(data).includes(sha));
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
    await check('GitHub clone pinned revision positive', async () => {
      const data = await call('ghCloneRepo', { ...repo, branch: sha });
      assert.ok(data.location.localPath);
      assert.equal(data.location.commitSha, sha);
      assert.equal(
        await readFile(path.join(data.location.localPath, 'README'), 'utf8'),
        'Hello World!\n'
      );
    });
  }
  if (values['cli-mcp-parity']) {
    await check('same-query CLI and real MCP structured result parity', async () => {
      const parity = [];
      receipt.cliMcpParity = parity;
      const cacheVolatileTools = new Set([
        'ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'artifactSearch',
      ]);
      for (const name of expectedTools) {
        const sample = cliMcpParitySamples.get(name);
        const selected = sample?.selected ?? receipt.calls.find(call =>
          call.name === name
          && call.response.isError === false
          && call.response.structuredContent?.results?.[0]?.status !== 'error'
          && call.response.structuredContent?.results?.[0]?.status !== 'empty'
          && (name !== 'astRewrite' || call.arguments.queries[0].apply !== true)
        );
        assert.ok(selected, `${name}: no successful non-mutating MCP call to compare`);
        const cliResponse = sample?.cliResponse ?? executeCliTool(name, selected.arguments.queries);
        const cliResults = sample?.cliResults ?? cliResponse.results;
        const mcpResults = selected.response.structuredContent.results;
        const mcpBase = selected.response.structuredContent.base;
        const cliBase = cliResponse?.base;
        let cloneVerification;
        if (name === 'ghCloneRepo') {
          const mcpLocation = mcpResults[0]?.data?.location;
          const cliLocation = cliResults[0]?.data?.location;
          assert.ok(mcpLocation?.localPath && cliLocation?.localPath, 'ghCloneRepo: missing warmed checkout location');
          assert.equal(mcpLocation.commitSha, cliLocation.commitSha, 'ghCloneRepo: commit identity differs');
          const [mcpBytes, cliBytes] = await Promise.all([
            readFile(path.join(mcpLocation.localPath, 'README')),
            readFile(path.join(cliLocation.localPath, 'README')),
          ]);
          assert.deepEqual(mcpBytes, cliBytes, 'ghCloneRepo: checkout source bytes differ');
          cloneVerification = {
            authorizedContext: 'replayed the exact base-live pinned clone query against an already warmed persistent cache',
            commitSha: mcpLocation.commitSha,
            mcpLocalPath: mcpLocation.localPath,
            cliLocalPath: cliLocation.localPath,
            readmeSha256: (await import('node:crypto')).createHash('sha256').update(mcpBytes).digest('hex'),
          };
        }
        const differences = differingFields(mcpResults, cliResults);
        // Provider cache state depends on which cross-process arm reached the
        // provider first. For these five provider tools it is receipt metadata,
        // not evidence or tool data; retain the raw pair and compare all other
        // fields explicitly. No local/AST/LSP result receives this exception.
        const cacheOnly = cacheVolatileTools.has(name) && differences.every(field => field === '/0/cache');
        const mcpComparable = cacheOnly ? mcpResults.map(({ cache, ...row }) => row) : mcpResults;
        const cliComparable = cacheOnly ? cliResults.map(({ cache, ...row }) => row) : cliResults;
        const contractDifferences = differingFields(mcpComparable, cliComparable);
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
          comparison: cacheOnly ? 'evidence-and-data-with-cache-excluded' : 'exact-structured-results',
          cacheExclusionJustification: cacheOnly ? 'provider cache state is cross-process timing metadata; all evidence and data fields remain exact' : undefined,
          differingFields: contractDifferences,
          passesContract: contractDifferences.length === 0 && baseEqual,
          cloneVerification,
        });
      }
      const failures = parity.filter(row => row.status !== 'not-run' && !row.passesContract);
      assert.deepEqual(failures.map(row => ({ name: row.name, baseEqual: row.baseEqual, differingFields: row.differingFields })), []);
    });
  }
  await check('stdio contains no parser or protocol errors', () =>
    assert.deepEqual(receipt.transportErrors, [])
  );
} finally {
  const start = Date.now();
  await client.close();
  receipt.shutdownMs = Date.now() - start;
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
    receipt: values.receipt,
    shutdownMs: receipt.shutdownMs,
  })
);
if (failures.length) process.exitCode = 1;
