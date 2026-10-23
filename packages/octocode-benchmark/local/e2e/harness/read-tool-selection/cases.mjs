// Explicit acceptance catalog for the thirteen evidence-reading tools.
// These are labeled controlled fixtures, not a representative workload sample.
const sourceTarget =
  'Find the rule that cancels a queued request before its work starts.';
const greetingTarget =
  'Find how a configured prefix is prepended to greeting text.';
const gh = { owner: 'octo', repo: 'demo' };

export const CASES = [
  {
    tool: 'localFetch',
    scope: 'selectedView',
    unit: 'chunks',
    query: { path: 'selection-guide.txt', unit: 'lines', length: 15 },
    classifiers: [sourceTarget],
    target: 'SELECTED_SELECTION_SENTINEL',
    control: {
      path: 'selection-guide.txt',
      matchString: 'Cancellation removes',
      contextLines: 0,
    },
    single: { path: 'single-selected.txt', unit: 'lines', length: 20 },
    protected: [
      { path: 'selection-guide.txt', unit: 'bytes', length: 100 },
      { path: 'selection-guide.txt', minify: 'standard' },
    ],
    kind: 'source',
  },
  {
    tool: 'localSearch',
    scope: 'selectedView',
    unit: 'chunks',
    query: {
      path: 'src',
      matchString: 'healthMarker',
      contextLines: 2,
      pageSize: 1,
      matchPageSize: 100,
    },
    classifiers: [greetingTarget],
    target: 'selected-selection.ts',
    control: {
      path: 'src',
      matchString: 'return this.prefix',
      contextLines: 0,
    },
    single: {
      path: 'src/selected-selection.ts',
      matchString: 'healthMarker',
      contextLines: 1,
    },
    protected: [
      { path: 'src', matchString: 'healthMarker', resultView: 'countMatches' },
    ],
    kind: 'files',
  },
  {
    tool: 'structureSearch',
    scope: null,
    query: { path: 'src', operation: 'files' },
    classifiers: ['Find the file likely to define the greeter implementation.'],
    target: 'greeter.ts',
    control: { path: 'src', operation: 'files', include: ['greeter.ts'] },
    protected: [
      { path: 'src', operation: 'tree' },
      { path: 'src', operation: 'files', pageSize: 1 },
    ],
    kind: 'structure',
    metadataOnly: true,
  },
  {
    tool: 'astSearch',
    scope: null,
    query: { path: 'src', operation: 'symbols' },
    classifiers: [
      'Find the class that implements configurable greeting behavior.',
    ],
    target: 'Greeter',
    control: { path: 'src/greeter.ts', operation: 'symbols' },
    protected: [
      { path: 'src/greeter.ts', operation: 'syntaxTree' },
      { path: 'src', operation: 'symbols', pageSize: 1 },
      {
        path: 'src',
        operation: 'match',
        pattern: 'greet($A)',
        matchPageSize: 1,
      },
    ],
    kind: 'files',
    verification: {
      tool: 'localFetch',
      kind: 'source',
      query: { path: 'src/greeter.ts', unit: 'lines', offset: 10, length: 8 },
    },
  },
  {
    tool: 'lspSearch',
    scope: null,
    query: {
      path: 'src/greeter.ts',
      symbolName: 'greet',
      line: 6,
      operation: 'references',
      contextLines: 2,
    },
    classifiers: [
      'Find a greeting caller that converts the result to uppercase.',
    ],
    target: 'greetLoudly',
    control: {
      path: 'src/app.ts',
      symbolName: 'greet',
      line: 6,
      operation: 'definition',
    },
    protected: [
      {
        path: 'src/greeter.ts',
        symbolName: 'greet',
        line: 6,
        operation: 'definition',
      },
      {
        path: 'src/greeter.ts',
        symbolName: 'greet',
        line: 6,
        operation: 'references',
        pageSize: 1,
      },
    ],
    kind: 'files',
    needsLsp: true,
    verification: {
      tool: 'localFetch',
      kind: 'source',
      query: { path: 'src/util.ts', unit: 'lines', offset: 3, length: 7 },
    },
  },
  {
    tool: 'ghSearchRepo',
    scope: null,
    query: { keywords: ['demo'], pageSize: 2 },
    classifiers: ['Find the repository that documents the demo greeter.'],
    target: 'demo-docs',
    control: { owner: 'octo', repo: 'demo-docs' },
    protected: [{ keywords: ['demo'], sort: 'stars:desc' }],
    kind: 'repositories',
    metadataOnly: true,
  },
  {
    tool: 'ghSearchCode',
    scope: 'providerPage',
    unit: 'files',
    query: { ...gh, keywords: ['healthMarker'], pageSize: 2 },
    classifiers: [greetingTarget],
    target: 'selected-selection.ts',
    control: {
      ...gh,
      keywords: ['return this.prefix'],
      path: 'src/greeter.ts',
    },
    kind: 'files',
    single: {
      ...gh,
      keywords: ['healthMarker'],
      path: 'src/selected-selection.ts',
      pageSize: 10,
    },
    narrow: {
      ...gh,
      keywords: ['healthMarker'],
      fields: ['path'],
      pageSize: 3,
    },
    metadataSentinel: 'BODY_ONLY_SENTINEL',
    verification: {
      tool: 'ghGetFileContent',
      kind: 'source',
      query: {
        ...gh,
        path: 'src/greeter.ts',
        ref: '$fixtureHead',
        unit: 'lines',
        offset: 10,
        length: 8,
      },
    },
  },
  {
    tool: 'ghStructure',
    scope: null,
    query: { ...gh, maxDepth: 3 },
    classifiers: ['Find the greeting implementation directory.'],
    target: 'src',
    control: { ...gh, path: 'src', maxDepth: 1 },
    kind: 'structural',
    skip: 'Tree, refs, and language structure are not independent ranking units.',
  },
  {
    tool: 'ghGetFileContent',
    scope: 'selectedView',
    unit: 'chunks',
    query: {
      ...gh,
      path: 'selection-guide.txt',
      unit: 'lines',
      length: 15,
    },
    classifiers: [sourceTarget],
    target: 'SELECTED_SELECTION_SENTINEL',
    control: {
      ...gh,
      path: 'selection-guide.txt',
      matchString: 'Cancellation removes',
      contextLines: 0,
    },
    single: {
      ...gh,
      path: 'single-selected.txt',
      unit: 'lines',
      length: 20,
    },
    protected: [
      { ...gh, path: 'selection-guide.txt', unit: 'bytes', length: 100 },
      { ...gh, path: 'selection-guide.txt', minify: 'standard' },
    ],
    kind: 'source',
  },
  {
    tool: 'ghSearchHistory',
    scope: null,
    query: {
      ...gh,
      operation: 'pullRequest',
      keywords: ['greeter'],
      pageSize: 2,
    },
    classifiers: [
      'Find the change that introduced the greeter implementation.',
    ],
    target: 'Add greeter module',
    control: { ...gh, operation: 'pullRequest', keywords: ['module'] },
    kind: 'pullRequests',
    metadataOnly: true,
    protected: [
      { ...gh, operation: 'pullRequest', pageSize: 2 },
      { ...gh, operation: 'issue' },
      {
        ...gh,
        operation: 'pullRequest',
        keywords: ['greeter'],
        sort: 'created:desc',
      },
    ],
    unsupported: [
      { query: { ...gh, operation: 'commit', pageSize: 2 }, kind: 'commits' },
    ],
    verification: {
      tool: 'ghGetHistoryItem',
      kind: 'structural',
      query: {
        ...gh,
        operation: 'pullRequest',
        number: 1,
        sections: ['patches'],
        include: ['src/greeter.ts'],
      },
    },
  },
  {
    tool: 'ghGetHistoryItem',
    scope: null,
    query: { ...gh, operation: 'pullRequest', number: 3, sections: ['files'] },
    classifiers: ['Find changes affecting greeting behavior.'],
    target: 'greeter',
    control: {
      ...gh,
      operation: 'pullRequest',
      number: 3,
      sections: ['patches'],
      include: ['src/greeter.ts'],
    },
    kind: 'structural',
    skip: 'Section pagination and patch/hunk order are protected.',
  },
  {
    tool: 'artifactSearch',
    scope: null,
    query: { ecosystem: 'npm', keywords: ['health'], pageSize: 2 },
    classifiers: ['Find a package for monitoring pulses in health checks.'],
    target: 'tiny-pulse',
    control: { ecosystem: 'npm', packageName: 'tiny-pulse' },
    protected: [
      { ecosystem: 'npm', packageName: 'tiny-health', version: '1.2.3' },
    ],
    kind: 'artifacts',
    metadataOnly: true,
  },
];

export const READ_TOOLS = CASES.map(row => row.tool);
export const materialize = (query, ctx) => ({
  ...query,
  ...(query.ecosystem === 'npm' ? { registryUrl: `${ctx.mock.url}/` } : {}),
  ...(query.ref === '$fixtureHead' ? { ref: ctx.mock.model.head.oid } : {}),
});
