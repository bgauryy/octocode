/** Real built-server acceptance. Run after building CLI + MCP; no mocks or installs. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';

const { values } = parseArgs({
  options: {
    server: { type: 'string', default: 'packages/octocode-mcp/dist/index.js' },
    cli: { type: 'string', default: 'packages/octocode/out/octocode.js' },
    cwd: { type: 'string', default: process.cwd() },
    node: { type: 'string', default: process.execPath },
    fixture: { type: 'string', default: '.octocode/tmp/preproduction-local' },
    receipt: {
      type: 'string',
      default: '.octocode/octocode-research/preproduction-mcp-receipt.json',
    },
    quick: { type: 'boolean', default: false },
    live: { type: 'boolean', default: false },
  },
});
const fixture = path.resolve(values.fixture);
const expectedTools = [
  'ghSearch',
  'ghGetFileContent',
  'ghSearchHistory',
  'ghGetHistoryItem',
  'ghCloneRepo',
  'npmSearch',
  'localSearch',
  'localGetFileContent',
  'localAnalyzeGraph',
  'lspGetSemantics',
];
const receipt = {
  server: path.resolve(values.server),
  node: values.node,
  fixture,
  checks: [],
  calls: [],
  transportErrors: [],
  stderrBytes: 0,
};
const transport = new StdioClientTransport({
  command: values.node,
  args: [path.resolve(values.server)],
  cwd: path.resolve(values.cwd),
  env: {
    ...process.env,
    ENABLE_LOCAL: 'true',
    ENABLE_CLONE: 'true',
    OCTOCODE_STORAGE_MODE: 'persistent',
  },
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
  await check('initialize and list all ten tools', () =>
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
          { encoding: 'utf8', timeout: 10_000 }
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
      const data = await call('localGetFileContent', {
        path: file,
        minify: 'none',
      });
      assert.equal(data.content, await readFile(file, 'utf8'));
      assert.ok(
        receipt.calls
          .at(-1)
          .response.content.some(
            block => block.type === 'text' && block.text.includes(data.content)
          )
      );
    }
  );
  await check('local file discovery positive', async () => {
    const data = await call('localSearch', {
      operation: 'files',
      path: fixture,
      extensions: ['ts'],
      pageSize: 50,
    });
    assert.ok(data.files.some(file => file.path.endsWith('math.ts')));
  });
  if (!values.quick) {
    await check(
      'outer text pagination preserves structured data and reconstructs every character',
      async () => {
        const args = {
          queries: [{ path: path.join(fixture, 'math.ts'), minify: 'none' }],
        };
        const full = await invoke('localGetFileContent', args);
        let current = await invoke('localGetFileContent', {
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
        const full = await call('localSearch', { ...query, pageSize: 50 });
        const first = await call('localSearch', { ...query, pageSize: 1 });
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
    await check('structural captures positive', async () => {
      const data = await call('localSearch', {
        operation: 'structural',
        path: fixture,
        pattern: 'add($$$ARGS)',
        langType: 'typescript',
        captureText: true,
      });
      assert.ok(JSON.stringify(data).includes('add(value, value)'));
    });
    await check('file graph dependency positive', async () => {
      const data = await call('localAnalyzeGraph', {
        operation: 'dependencies',
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
          operation: 'dependencies',
          path: `${fixture}-diagnostics`,
          file: 'entry.ts',
          depth: 3,
          pageSize: 50,
          excludeDir: [],
        };
        const full = await call('localAnalyzeGraph', {
          ...query,
          diagnosticPageSize: 100,
        });
        const first = await call('localAnalyzeGraph', {
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
      const data = await call('lspGetSemantics', {
        uri: path.join(fixture, 'entry.ts'),
        workspaceRoot: fixture,
        type: 'definition',
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
          type: 'references',
          symbolName: 'add',
          lineHint: 2,
        };
        const first = await call('lspGetSemantics', { ...query, pageSize: 1 });
        const paged = await pages(
          first,
          'nextPage',
          data => data.payload.locations
        );
        const full = await call('lspGetSemantics', { ...query, pageSize: 100 });
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
          assert.ok((await client.listTools()).tools.length === 10);
        }
      );
    await check(
      'unknown tool returns an error and server remains responsive',
      async () => {
        let rejected = false;
        try {
          const result = await client.callTool({
            name: 'octocode_nonexistent_tool',
            arguments: {},
          });
          rejected = result.isError === true;
        } catch {
          rejected = true;
        }
        assert.ok(rejected);
        assert.equal((await client.listTools()).tools.length, 10);
      }
    );
  }
  if (values.live && !values.quick) {
    const repo = { owner: 'octocat', repo: 'Hello-World' };
    const sha = '7fd1a60b01f91b314f59955a4e4d4e80d8edf11d';
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
      'GitHub exact file character continuations preserve all bytes',
      async () => {
        const query = { ...repo, branch: sha, path: 'README', minify: 'none' };
        const full = await call('ghGetFileContent', query);
        let current = await call('ghGetFileContent', {
          ...query,
          charLength: 5,
        });
        let content = current.files[0].content;
        let count = 1;
        while (current.files[0].next?.continueChars) {
          assert.ok(count++ < 20);
          current = await nextCall(current.files[0].next.continueChars);
          content += current.files[0].content;
        }
        assert.ok(count > 1);
        assert.equal(content, full.files[0].content);
      }
    );
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
      const data = await call('npmSearch', { packageName: 'is-number' });
      assert.ok(JSON.stringify(data).includes('7.0.0'));
    });
    await check('npm discovery continuation is executable', async () => {
      const data = await call('npmSearch', {
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
