// AST/LSP navigation and malformed-call resilience checks for
// stdio.acceptance.mjs: the server answers bad input as a tool error and
// keeps serving tools/list.
import assert from 'node:assert/strict';
import path from 'node:path';
import {
  call,
  client,
  fixture,
  invoke,
  pages,
  receipt,
  state,
} from './stdio-harness.mjs';

async function structuralCaptures() {
  const data = await call('astSearch', {
    operation: 'match',
    path: fixture,
    pattern: 'add($$$ARGS)',
    language: 'typescript',
    captureText: true,
  });
  assert.ok(JSON.stringify(data).includes('add(value, value)'));
}

async function lspDefinition() {
  const data = await call('lspSearch', {
    path: path.join(fixture, 'entry.ts'),
    workspaceRoot: fixture,
    operation: 'definition',
    symbolName: 'add',
    line: 4,
  });
  // A `kind` that restates the operation asked is compacted away; the
  // location row itself identifies the declaration.
  assert.equal(Object.hasOwn(data, 'kind'), false);
  assert.ok(
    data.matches.some(
      location =>
        location.path.endsWith('math.ts') &&
        location.displayRange.startLine === 2 &&
        location.content.startsWith('2\texport function add(')
    )
  );
}

// References group per file: {path, matches: [{line, column, value}]}.
const collectReferences = data =>
  data.files.flatMap(file =>
    file.matches.map(
      ref => `${file.path} ${ref.line}:${ref.column} ${ref.value}`
    )
  );

async function lspReferencePages() {
  const query = {
    path: path.join(fixture, 'math.ts'),
    workspaceRoot: fixture,
    operation: 'references',
    symbolName: 'add',
    line: 2,
  };
  const first = await call('lspSearch', { ...query, pageSize: 1 });
  const paged = await pages(first, 'nextPage', collectReferences);
  const full = await call('lspSearch', { ...query, pageSize: 100 });
  assert.ok(paged.count > 1);
  assert.equal(
    paged.rows.length,
    full.pagination?.totalItems ?? collectReferences(full).length
  );
  assert.deepEqual(paged.rows, collectReferences(full));
}

const rejectsMalformedArguments = name => [
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
    assert.ok(
      message.length > 0 && message.length <= 2_000,
      `${name}: unusable validation message`
    );
    assert.equal(
      (await client.listTools()).tools.length,
      state.expectedTools.length
    );
  },
];

const rejectsRemovedTool = removedName => [
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
    assert.equal(
      (await client.listTools()).tools.length,
      state.expectedTools.length
    );
  },
];

/** The navigation and resilience checks, in run order (skipped by --quick). */
export const navigationChecks = () => [
  ['structural captures positive', structuralCaptures],
  ['LSP definition identifies the declaration', lspDefinition],
  [
    'LSP references execute snapshot continuations without loss',
    lspReferencePages,
  ],
  ...state.expectedTools.map(rejectsMalformedArguments),
  ...['localAnalyzeGraph', 'lspGetSemantics', 'octocode_nonexistent_tool'].map(
    rejectsRemovedTool
  ),
];
