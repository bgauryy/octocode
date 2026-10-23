// Provider-backed checks for stdio.acceptance.mjs (--live without --quick):
// GitHub, npm, the CLI-only clone, and clasify through the live provider.
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  call,
  executeCliTool,
  fixture,
  invoke,
  nextCall,
  state,
  suggestion,
  byCodeUnit,
} from './stdio-harness.mjs';

const { TOOL_NAMES } = await import('@octocodeai/config/schema');
const repo = { owner: 'octocat', repo: 'Hello-World' };
const sha = '7fd1a60b01f91b314f59955a4e4d4e80d8edf11d';

async function clasifyPrimitives() {
  const response = await invoke(TOOL_NAMES.CLASIFY, {
    queries: [
      {
        id: 'live-primitives',
        state: 'The fixture explicitly states that alpha is enabled.',
        questions: {
          stated: {
            type: 'noul',
            instructions: 'Does the fixture state that alpha is enabled?',
          },
          state: {
            type: 'choice',
            instructions: 'Which state does the fixture assign to alpha?',
            criteria: {
              enabled: 'The fixture says alpha is enabled.',
              disabled: 'The fixture says alpha is disabled.',
            },
          },
          explicit: {
            type: 'score',
            instructions:
              'How explicit is the fixture about alpha being enabled?',
            criteria: ['not stated', 'implied', 'explicitly stated'],
          },
        },
      },
    ],
  });
  assert.equal(response.isError, false, JSON.stringify(response));
  const row = response.structuredContent?.queries?.[0];
  assert.equal(row?.id, 'live-primitives');
  assert.equal(typeof row.model, 'string');
  assert.equal(typeof row.usage?.input_tokens, 'number');
  assert.equal(row.answers.stated.type, 'noul');
  assert.equal(typeof row.answers.stated.noul, 'number');
  assert.equal(row.answers.state.type, 'choice');
  assert.ok(['enabled', 'disabled'].includes(row.answers.state.choice));
  assert.equal(row.answers.explicit.type, 'score');
  assert.equal(typeof row.answers.explicit.score, 'number');
  // A literal state needs no source-read lead.
  assert.equal(row.hints, undefined);
}

async function githubTree() {
  const data = await call('ghStructure', { ...repo, ref: sha, pageSize: 1 });
  // The resolved ref restates the requested ref, so it is not repeated.
  assert.equal(data.resolvedRef ?? sha, sha);
  assert.ok(data.entries.some(directory => directory.files.includes('README')));
}

async function githubFullRead() {
  const data = await call('ghGetFileContent', {
    ...repo,
    ref: sha,
    path: 'README',
    fullContent: true,
  });
  assert.equal(data.content, '1\tHello World!\n');
  assert.equal(data.files, undefined);
  assert.equal(data.localPath, undefined);
  assert.equal(data.repoRoot, undefined);
  assert.equal(data.directories, undefined);
}

async function githubRejectsDirectories() {
  const directory = await invoke('ghGetFileContent', {
    queries: [{ ...repo, ref: sha, path: '' }],
  });
  assert.equal(directory.isError, true);
  assert.match(
    JSON.stringify(
      directory.structuredContent ?? directory.content ?? directory
    ),
    /non-blank path|directory/i
  );
  let rejected = false;
  try {
    const result = await invoke('ghGetFileContent', {
      queries: [{ ...repo, path: '', type: 'directory' }],
    });
    rejected = result.isError === true;
  } catch {
    rejected = true;
  }
  assert.ok(rejected);
}

async function githubByteContinuations() {
  const query = { ...repo, ref: sha, path: 'README' };
  const full = await call('ghGetFileContent', query);
  let current = await call('ghGetFileContent', {
    ...query,
    unit: 'bytes',
    length: 5,
  });
  let content = current.content;
  let count = 1;
  while (current.next?.continueWindow) {
    assert.ok(count++ < 20);
    current = await nextCall(current.next.continueWindow);
    content += current.content;
  }
  assert.ok(count > 1);
  assert.equal(content, full.content);
}

/** Joins every continueWindow page of one match-evidence fetch. */
async function readMatchEvidence(tool, query, source, matchString) {
  let result = await call(tool, query);
  let joined = '';
  let count = 0;
  const matched = new Set();
  while (true) {
    assert.ok(count++ < 100);
    const page = result;
    assert.equal(page.sourceBytes, Buffer.byteLength(source));
    assert.equal(page.returnedBytes, Buffer.byteLength(page.content));
    assert.equal(page.minifyFallback.reason, 'match-evidence');
    joined += page.content;
    for (const line of page.matchedLines ?? []) matched.add(line);
    if (!page.next?.continueWindow) {
      assert.equal(page.pagination.hasMore, false);
      break;
    }
    assert.equal(
      page.next.continueWindow.query.queries[0].matchString,
      matchString
    );
    result = await nextCall(page.next.continueWindow);
  }
  assert.ok(count > 1);
  return { joined, anchors: [...matched] };
}

const isNumberRepo = { owner: 'jonschlinkert', repo: 'is-number' };

/** Local and remote fetches of one source agree page for page, per unit. */
async function compareChunkUnits(file, remotePath, full) {
  for (const unit of ['lines', 'bytes']) {
    const selector = {
      matchString: 'module.exports',
      contextLines: 2,
      minify: 'standard',
      unit,
      length: unit === 'lines' ? 1 : 7,
      extras: ['stats', 'details'],
    };
    const contents = [];
    const anchors = [];
    for (const remote of [false, true]) {
      const tool = remote ? 'ghGetFileContent' : 'localFetch';
      const query = remote
        ? {
            ...isNumberRepo,
            path: remotePath,
            ...(full.commitSha ? { ref: full.commitSha } : {}),
            ...selector,
          }
        : { path: file, ...selector };
      const read = await readMatchEvidence(
        tool,
        query,
        full.content,
        selector.matchString
      );
      contents.push(read.joined);
      anchors.push(read.anchors);
    }
    assert.equal(contents[0], contents[1]);
    assert.deepEqual(anchors[0], anchors[1]);
    assert.ok(anchors[0].length > 0);
  }
}

async function searchToMatchParity() {
  const found = await call('ghSearchCode', {
    ...isNumberRepo,
    filename: 'index.js',
    keywords: ['module.exports'],
    pageSize: 1,
  });
  assert.ok(found.files?.length > 0);
  const remotePath = found.files[0].path;
  const full = await call('ghGetFileContent', {
    ...isNumberRepo,
    path: remotePath,
    fullContent: true,
  });
  const source = full.content;
  assert.ok(source.includes('module.exports'));
  const parent = fixture;
  const directory = await mkdtemp(path.join(parent, 'remote-local-fetch-'));
  const file = path.join(directory, 'index.js');
  try {
    await writeFile(file, source);
    const localFound = await call('localSearch', {
      path: directory,
      matchString: 'module.exports',
      regex: 'literal',
    });
    assert.ok(localFound.files?.length > 0);
    await compareChunkUnits(file, remotePath, full);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

async function githubCodeSearch() {
  const data = await call('ghSearchCode', {
    ...isNumberRepo,
    filename: 'index.js',
    keywords: ['module.exports'],
    pageSize: 1,
  });
  assert.ok(data.files.length > 0);
}

async function githubHistoryPages() {
  const data = await call('ghSearchHistory', {
    ...repo,
    operation: 'commit',
    pageSize: 1,
  });
  assert.ok(JSON.stringify(data).includes(sha));
  assert.ok(data.next?.nextPage);
  const next = await nextCall(data.next.nextPage);
  assert.equal(next.pagination.currentPage, 2);
}

async function npmDiscoveryPage() {
  const data = await call('artifactSearch', {
    ecosystem: 'npm',
    keywords: ['is-number'],
    pageSize: 1,
  });
  assert.ok(data.next?.nextPage);
  await nextCall(data.next.nextPage);
}

async function cliClone() {
  const data = executeCliTool('ghCloneRepo', [
    { ...repo, ref: sha, reasoning: 'Verify CLI-only clone.' },
  ]).results[0].data;
  assert.ok(data.location.localPath);
  assert.equal(data.location.commitSha, sha);
  assert.equal(
    await readFile(path.join(data.location.localPath, 'README'), 'utf8'),
    'Hello World!\n'
  );
}

async function clasifySourceReaders() {
  const sourceSchema = state.list.tools.find(
    tool => tool.name === TOOL_NAMES.CLASIFY
  ).inputSchema.properties.queries.items.properties.source;
  const readers = sourceSchema.properties.tool.enum;
  const sources = {
    localFetch: { path: path.join(fixture, 'math.ts'), ranges: ['1-3'] },
    ghGetFileContent: { ...repo, path: 'README', ref: sha, fullContent: true },
  };
  assert.deepEqual(
    [...readers].toSorted(byCodeUnit),
    Object.keys(sources).toSorted(byCodeUnit)
  );
  for (const tool of readers) {
    const response = await invoke(TOOL_NAMES.CLASIFY, {
      queries: [
        {
          id: tool,
          source: { tool, query: sources[tool] },
          questions: {
            named: {
              type: 'noul',
              instructions:
                'Does this source name a file, repository, package, or code symbol?',
            },
          },
        },
      ],
    });
    assert.equal(response.isError, false, JSON.stringify(response));
    const row = response.structuredContent.queries[0];
    assert.equal(row.error, undefined, JSON.stringify(row));
    const answer = row.answers.named.noul;
    assert.ok(answer >= 0 && answer <= 1, JSON.stringify(row));
    assert.equal(JSON.stringify(row).includes('Hello World'), false);
    const lead = suggestion(row, 'read');
    assert.equal(lead?.tool, tool, JSON.stringify(row));
    const read = await nextCall(lead);
    assert.ok(String(read.content ?? '').length > 0, tool);
  }
}

const positive = (name, tool, query, expected) => [
  name,
  async () => {
    const data = await call(tool, query);
    assert.ok(JSON.stringify(data).includes(expected));
  },
];

/** The live checks, in run order. */
export const liveChecks = () => {
  const clasify = state.expectedTools.includes(TOOL_NAMES.CLASIFY);
  return [
    ...(clasify
      ? [
          [
            'clasify executes noul, choice, and score through the live provider',
            clasifyPrimitives,
          ],
        ]
      : []),
    ['GitHub tree preserves paths and the requested revision', githubTree],
    [
      'GitHub full file reads return content without a checkout',
      githubFullRead,
    ],
    [
      'GitHub file fetch rejects directory paths and removed directory mode',
      githubRejectsDirectories,
    ],
    positive(
      'GitHub repository search positive',
      'ghSearchRepo',
      { owner: 'octocat', keywords: ['Hello-World'], pageSize: 1 },
      'Hello-World'
    ),
    ['GitHub code search positive', githubCodeSearch],
    [
      'GitHub exact file byte continuations preserve all bytes',
      githubByteContinuations,
    ],
    [
      'GitHub/local search-to-match fetch parity in both chunk units',
      searchToMatchParity,
    ],
    ['GitHub commit history search positive', githubHistoryPages],
    positive(
      'GitHub exact commit positive',
      'ghGetHistoryItem',
      { ...repo, operation: 'commit', ref: sha, sections: ['patches'] },
      sha
    ),
    positive(
      'npm exact metadata positive',
      'artifactSearch',
      { ecosystem: 'npm', packageName: 'is-number' },
      '7.0.0'
    ),
    ['npm discovery continuation is executable', npmDiscoveryPage],
    ['CLI clone pinned revision positive', cliClone],
    ...(clasify
      ? [
          [
            'clasify judges each source reader without returning a body, and its read lead returns the captured source',
            clasifySourceReaders,
          ],
        ]
      : []),
  ];
};
