import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import path from 'node:path';

const runOqlSearch = vi.fn();
const oqlSchemaText = vi.fn(() => '{"schema":"oql"}');
const outlineSymbols = vi.fn();
const materializeRemoteForCli = vi.fn();
// Pass-through spy: the real secret scanner is aliased to a no-op stub in the
// CLI vitest config (no native binary), so we verify the command INVOKES
// sanitization on the envelope; real redaction is covered by the tools-core
// responses test + the live E2E.
const sanitizeStructuredContentSpy = vi.fn((x: unknown) => x);

vi.mock('@octocodeai/octocode-tools-core/oql', async () => {
  // keep the real shorthand lowering (tools-core owns it); mock only execution
  const actual = await vi.importActual<
    typeof import('@octocodeai/octocode-tools-core/oql')
  >('@octocodeai/octocode-tools-core/oql');
  return {
    ...actual,
    runOqlSearch: (...args: unknown[]) => runOqlSearch(...args),
    oqlSchemaText: () => oqlSchemaText(),
    sanitizeStructuredContent: (x: unknown) => sanitizeStructuredContentSpy(x),
  };
});

vi.mock('../../../src/cli/commands/symbol-outline.js', () => ({
  outlineSymbols: (...args: unknown[]) => outlineSymbols(...args),
}));

vi.mock('../../../src/cli/remote-local.js', () => ({
  materializeRemoteForCli: (...args: unknown[]) =>
    materializeRemoteForCli(...args),
  formatMaterializationHints: () => 'location:\n  localPath: "/tmp/repo"',
  withMaterializationHints: (
    result: { structuredContent?: Record<string, unknown> },
    materialized: { localPath: string }
  ) => ({
    ...result,
    structuredContent: {
      ...(result.structuredContent ?? {}),
      location: { localPath: materialized.localPath },
    },
  }),
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  bold: (s: string) => s,
  dim: (s: string) => s,
}));

import { searchCommand } from '../../../src/cli/commands/search.js';
import { EXIT } from '../../../src/cli/exit-codes.js';
import type { ParsedArgs } from '../../../src/cli/types.js';

function run(
  options: Record<string, string | boolean> = {},
  args: string[] = []
) {
  const parsed: ParsedArgs = { command: 'search', args, options };
  return searchCommand.handler(parsed);
}

let stdout: string;

beforeEach(() => {
  stdout = '';
  vi.spyOn(process.stdout, 'write').mockImplementation((chunk: unknown) => {
    stdout += String(chunk);
    return true;
  });
  process.exitCode = 0;
  runOqlSearch.mockReset();
  outlineSymbols.mockReset();
  materializeRemoteForCli.mockReset();
});

afterEach(() => {
  vi.restoreAllMocks();
  process.exitCode = 0;
});

function proofEnvelope() {
  return {
    results: [
      {
        kind: 'code',
        source: { kind: 'local', path: '.' },
        path: 'a.ts',
        line: 3,
        snippet: 'hit',
      },
    ],
    diagnostics: [],
    provenance: [
      { backend: 'localSearchCode', source: { kind: 'local', path: '.' } },
    ],
    evidence: { answerReady: true, complete: true, kind: 'proof' },
  };
}

function contentEnvelope() {
  return {
    results: [
      {
        kind: 'content',
        source: { kind: 'local', path: '.' },
        path: 'a.ts',
        contentView: 'none',
        content: 'export const value = 1;',
      },
    ],
    diagnostics: [],
    provenance: [
      { backend: 'localGetFileContent', source: { kind: 'local', path: '.' } },
    ],
    evidence: { answerReady: true, complete: true, kind: 'proof' },
  };
}

function recordEnvelope(
  recordType: string,
  data: Record<string, unknown>,
  id = 'row'
) {
  return {
    results: [
      {
        kind: 'record',
        recordType,
        id,
        data,
      },
    ],
    diagnostics: [],
    provenance: [],
    evidence: { answerReady: true, complete: true, kind: 'proof' },
  };
}

describe('octocode search command', () => {
  it('--scheme prints the OQL schema without running a query', async () => {
    await run({ scheme: true });
    expect(stdout).toContain('oql');
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('--scheme --compact prints the lean agent guide derived from the schema', async () => {
    // oqlCompactSchemeText is NOT mocked (only oqlSchemaText is), so this runs
    // the real renderer over OQL_SCHEMA_DOC.
    await run({ scheme: true, compact: true });
    expect(stdout).toContain('compact agent guide');
    expect(stdout).toContain('SOURCE');
    expect(stdout).toContain('TARGET');
    // npm + remote-file-read recipes (Haiku gaps) are surfaced
    expect(stdout).toContain('--target packages');
    expect(stdout).toContain('--content-view none');
    // references vs callers distinction
    expect(stdout).toContain('references');
    expect(stdout).toContain('callers');
    // points back to the full schema, and never runs a query
    expect(stdout).toContain('search --scheme');
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('--scheme --json --compact prints the lean agent guide as JSON', async () => {
    await run({ scheme: true, compact: true, json: true });
    const parsed = JSON.parse(stdout) as { kind: string; targets: unknown[] };
    expect(parsed.kind).toBe('octocode.search.compactScheme');
    expect(Array.isArray(parsed.targets)).toBe(true);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('sanitizes the OQL envelope before any output (no interface leaks secrets)', async () => {
    sanitizeStructuredContentSpy.mockClear();
    const envelope = {
      results: [
        {
          kind: 'code',
          source: { kind: 'local', path: '.' },
          path: 'a.ts',
          line: 1,
          snippet: 'const t = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";',
        },
      ],
      diagnostics: [],
      provenance: [
        { backend: 'localSearchCode', source: { kind: 'local', path: '.' } },
      ],
      evidence: { answerReady: true, complete: true, kind: 'proof' },
    };
    runOqlSearch.mockResolvedValue(envelope);
    await run({}, ['ghp', './src']);
    // The CLI must hand the raw OQL envelope to the sanitizer before printing —
    // the interface-layer redaction the MCP path gets via sanitizeCallToolResult.
    expect(sanitizeStructuredContentSpy).toHaveBeenCalledTimes(1);
    expect(sanitizeStructuredContentSpy).toHaveBeenCalledWith(envelope);
  });

  it('errors with USAGE exit when no query is provided', async () => {
    await run({});
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('rejects malformed JSON with USAGE exit', async () => {
    await run({ query: '{not json' });
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  it('runs a valid --query and renders results', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    expect(runOqlSearch).toHaveBeenCalledTimes(1);
    expect(stdout).toContain('a.ts');
    expect(process.exitCode).toBe(EXIT.OK);
  });

  it('--quiet prints result rows ONLY (no evidence footer, diagnostics, or continuations)', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({
      quiet: true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    expect(stdout).toContain('a.ts:3');
    expect(stdout).toContain('hit');
    expect(stdout).not.toContain('evidence:');
    expect(stdout).not.toContain('answerReady');
    expect(stdout).not.toContain('next.');
    expect(stdout).not.toContain('PLAN');
    expect(process.exitCode).toBe(EXIT.OK);
  });

  it('--quiet emits nothing on zero results', async () => {
    runOqlSearch.mockResolvedValue({
      ...proofEnvelope(),
      results: [],
    });
    await run({
      quiet: true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    expect(stdout).toBe('');
  });

  it('rejects --quiet combined with --json', async () => {
    await run({ quiet: true, json: true, query: '{}' });
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('accepts top-level array JSON as an OQL batch for --query', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({
      query:
        '[{"target":"structure","from":{"kind":"local","path":"src"}},{"target":"files","from":{"kind":"local","path":"src"}}]',
    });
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      queries: [
        { target: 'structure', from: { kind: 'local', path: 'src' } },
        { target: 'files', from: { kind: 'local', path: 'src' } },
      ],
    });
  });

  it('renders semantic document symbols in text output', async () => {
    runOqlSearch.mockResolvedValue(
      recordEnvelope('semantics', {
        type: 'documentSymbols',
        summary: {
          totalSymbols: 13,
          returnedSymbols: 13,
          kinds: { function: 10, class: 3 },
        },
        payload: {
          kind: 'documentSymbols',
          symbols: [
            { name: 'runCLI', kind: 'function', line: 73 },
            { name: 'SearchCommand', kind: 'class', line: 14 },
          ],
        },
      })
    );
    await run({
      query:
        '{"target":"semantics","from":{"kind":"local","path":"index.ts"},"params":{"type":"documentSymbols"}}',
    });
    expect(stdout).toContain('semantics row');
    expect(stdout).toContain('symbols=13/13');
    expect(stdout).toContain('function=10 class=3');
    expect(stdout).toContain('runCLI:73 function');
  });

  it('renders semantic locations in text output', async () => {
    runOqlSearch.mockResolvedValue(
      recordEnvelope('semantics', {
        type: 'references',
        resolvedSymbol: { name: 'runCLI', foundAtLine: 73 },
        payload: {
          kind: 'references',
          totalReferences: 1,
          totalFiles: 1,
          locations: [
            {
              uri: 'index.ts',
              content: 'runCLI();',
              displayRange: { startLine: 99 },
            },
          ],
        },
      })
    );
    await run({
      query:
        '{"target":"semantics","from":{"kind":"local","path":"index.ts"},"params":{"type":"references","symbolName":"runCLI","lineHint":73}}',
    });
    expect(stdout).toContain('references');
    expect(stdout).toContain('runCLI:73');
    expect(stdout).toContain('refs=1');
    expect(stdout).toContain('index.ts:99 runCLI();');
  });

  it('--explain sets explain:true on the query', async () => {
    runOqlSearch.mockResolvedValue({
      ...proofEnvelope(),
      plan: { nodes: [], normalized: {} },
    });
    await run({
      explain: true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    const [input] = runOqlSearch.mock.calls[0]!;
    expect((input as { explain?: boolean }).explain).toBe(true);
  });

  it('--json emits the raw envelope', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({
      json: true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    expect(() => JSON.parse(stdout)).not.toThrow();
  });

  it('--json without --compact stays pretty-printed (multi-line)', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({
      json: true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    // 2-space indentation is the pretty-print marker
    expect(stdout).toContain('\n  ');
  });

  it('--json --compact emits single-line minified JSON', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({
      json: true,
      compact: true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    const out = stdout.replace(/\n$/, '');
    // no raw newlines anywhere outside the single trailing one
    expect(out.includes('\n')).toBe(false);
    // valid and equal to the canonical minified form
    expect(() => JSON.parse(out)).not.toThrow();
    expect(out).toBe(JSON.stringify(JSON.parse(out)));
  });

  it('--json --compact keeps code-char content valid (newlines/quotes escaped, not raw)', async () => {
    runOqlSearch.mockResolvedValue({
      results: [
        {
          kind: 'content',
          source: { kind: 'local', path: '.' },
          path: 'a.ts',
          contentView: 'none',
          // embedded newline, tab, quote, backslash — the exact code-char case
          content: 'line1\nline2\t"q"\\end',
        },
      ],
      diagnostics: [],
      provenance: [
        {
          backend: 'localGetFileContent',
          source: { kind: 'local', path: '.' },
        },
      ],
      evidence: { answerReady: true, complete: true, kind: 'proof' },
    });
    await run({
      json: true,
      compact: true,
      query:
        '{"target":"content","from":{"kind":"local","path":"a.ts"},"fetch":{"content":{"contentView":"none"}}}',
    });
    const out = stdout.replace(/\n$/, '');
    // single line: the embedded newline is escaped (\n), never a raw line break
    expect(out.includes('\n')).toBe(false);
    const parsed = JSON.parse(out);
    // and the real newline round-trips back out of the string value
    expect(parsed.results[0].content).toBe('line1\nline2\t"q"\\end');
  });

  it('--dry-run passes dryRun to the runner', async () => {
    runOqlSearch.mockResolvedValue({ ...proofEnvelope(), results: [] });
    await run({
      'dry-run': true,
      query:
        '{"target":"code","from":{"kind":"local","path":"."},"where":{"kind":"text","value":"x"}}',
    });
    const [, opts] = runOqlSearch.mock.calls[0]!;
    expect((opts as { dryRun?: boolean }).dryRun).toBe(true);
  });

  it('renders compact research packet IDs and a copyable next.graph command', async () => {
    runOqlSearch.mockResolvedValue({
      results: [
        {
          kind: 'record',
          recordType: 'research',
          id: 'reachability',
          data: {
            intent: 'reachability',
            summary: { sourceFiles: 2, candidateUnusedExports: 1 },
            packets: [
              {
                subject: { id: 'sym:a.ts#deadFn' },
                verdict: 'candidate-dead',
                proofStatus: 'candidate',
              },
            ],
          },
          next: {
            'next.graph': {
              query: {
                schema: 'oql',
                target: 'graph',
                from: { kind: 'local', path: '/repo/src' },
                params: {
                  intent: 'reachability',
                  mode: 'prove',
                  proof: 'lsp',
                  proofLimit: 1,
                },
              },
            },
          },
        },
      ],
      nextHints: {
        'next.graph': {
          why: 'Upgrade this candidate research to LSP-proven relationships.',
          confidence: 'exact',
        },
      },
      diagnostics: [],
      provenance: [],
      evidence: { answerReady: false, complete: false, kind: 'candidate' },
    });

    await run(
      {
        compact: true,
        query:
          '{"target":"research","from":{"kind":"local","path":"/repo/src"},"params":{"intent":"reachability"}}',
      },
      []
    );

    expect(stdout).toContain(
      'packets=sym:a.ts#deadFn[candidate-dead/candidate]'
    );
    expect(stdout).toContain('next.graph');
    expect(stdout).toContain('search --query');
    expect(stdout).toContain('"target":"graph"');
  });

  it('renders copyable next.page and next.materialize commands in compact text', async () => {
    runOqlSearch.mockResolvedValue({
      results: [],
      next: {
        'next.page': {
          query: {
            schema: 'oql',
            target: 'code',
            from: { kind: 'local', path: '/repo/src' },
            page: 2,
          },
          why: 'More result pages remain.',
          confidence: 'exact',
        },
        'next.materialize': {
          query: {
            schema: 'oql',
            target: 'materialize',
            from: { kind: 'github', repo: 'facebook/react' },
            materialize: { mode: 'required' },
          },
          why: 'GitHub code search returned no results.',
          confidence: 'heuristic',
        },
      },
      diagnostics: [],
      provenance: [],
      evidence: { answerReady: false, complete: false, kind: 'partial' },
    });

    await run(
      {
        compact: true,
        query:
          '{"target":"code","from":{"kind":"github","repo":"facebook/react"},"where":{"kind":"text","value":"x"}}',
      },
      []
    );

    expect(stdout).toContain('next.page');
    expect(stdout).toContain('"page":2');
    expect(stdout).toContain('next.materialize');
    expect(stdout).toContain('"target":"materialize"');
  });

  it('unsupported evidence yields a TOOL exit code', async () => {
    runOqlSearch.mockResolvedValue({
      results: [],
      diagnostics: [
        {
          code: 'unsupportedTarget',
          severity: 'error',
          message: 'x',
          blocksAnswer: true,
        },
      ],
      provenance: [],
      evidence: { answerReady: false, complete: false, kind: 'unsupported' },
    });
    await run({ query: '{"target":"repositories","repo":"a/b"}' });
    expect(process.exitCode).toBe(EXIT.TOOL);
  });

  it('renders object-shaped diagnostic messages without crashing', async () => {
    runOqlSearch.mockResolvedValue({
      results: [],
      diagnostics: [
        {
          code: 'invalidQuery',
          severity: 'error',
          message: {
            error: 'Failed to fetch pull request #999999999',
            status: 404,
          },
          blocksAnswer: true,
        },
      ],
      provenance: [],
      evidence: { answerReady: false, complete: false, kind: 'partial' },
    });

    await expect(
      run({
        query:
          '{"target":"diff","from":{"kind":"github","repo":"facebook/react"},"params":{"prNumber":999999999}}',
      })
    ).resolves.toBeUndefined();
    expect(stdout).toContain('Failed to fetch pull request #999999999');
    expect(process.exitCode).toBe(EXIT.USAGE);
  });
});

describe('octocode search shorthand sugar', () => {
  it('positional text builds a code/text sugar object', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({}, ['runCLI', './src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'code',
      where: { kind: 'text', value: 'runCLI' },
    });
    expect(input.from).toMatchObject({ kind: 'local' });
  });

  it('routes a NON-EXISTENT file path + --content-view to content at that path (clean not-found, not a "." dir read)', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    const ghost = '/tmp/octocode-ghost-dir/missing-file.ts';
    await run({ 'content-view': 'symbols' }, [ghost]);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({ target: 'content' });
    // The named file must become the corpus path so localGetFileContent can
    // report a clean "File not found" — NOT fall back to cwd "." (a directory),
    // which produced the misleading "Path is a directory" error.
    const from = input.from as { kind?: string; path?: string };
    expect(from.path).toBe(path.resolve(ghost));
    expect(from.path).not.toBe('.');
  });

  it('lowers canonical text-search flags into search/OQL controls', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        lang: 'ts',
        view: 'discovery',
        context: '2',
        fixed: true,
        'max-matches': '3',
      },
      ['runCLI', './src']
    );
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'code',
      view: 'discovery',
      scope: { language: 'ts' },
      where: { kind: 'text', value: 'runCLI' },
      controls: {
        search: {
          contextLines: 2,
          maxMatchesPerFile: 3,
        },
      },
    });
  });

  it('lowers canonical regex PCRE mode with search --regex/--pcre2', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ regex: 'runCLI\\(', pcre2: true }, ['./src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'code',
      where: { kind: 'regex', value: 'runCLI\\(', dialect: 'pcre2' },
    });
  });

  it('--search both builds files + code OQL batch', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ search: 'both', ext: 'ts', entry: 'file' }, ['auth', './src']);

    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      combine: 'independent',
      queries: [
        {
          target: 'files',
          scope: { include: ['**/*.ts'] },
          where: {
            kind: 'all',
            of: expect.arrayContaining([
              { kind: 'field', field: 'entryType', op: '=', value: 'file' },
              expect.objectContaining({ kind: 'field' }),
            ]),
          },
        },
        {
          target: 'code',
          scope: { include: ['**/*.ts'] },
          where: {
            kind: 'all',
            of: expect.arrayContaining([
              { kind: 'field', field: 'entryType', op: '=', value: 'file' },
              { kind: 'field', field: 'extension', op: '=', value: 'ts' },
              { kind: 'text', value: 'auth' },
            ]),
          },
        },
      ],
    });
  });

  it('accepts --owner/--repo/--path GitHub scoping', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        source: 'github',
        owner: 'bgauryy',
        repo: 'octocode-mcp',
        path: 'packages/octocode/src',
        search: 'path',
      },
      ['auth']
    );

    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'files',
      from: { kind: 'github', repo: 'bgauryy/octocode-mcp' },
      scope: { path: 'packages/octocode/src' },
      where: expect.objectContaining({ kind: 'field' }),
    });
  });

  it('single existing local file positional builds a content read', async () => {
    runOqlSearch.mockResolvedValue(contentEnvelope());
    await run({ json: true }, ['package.json']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'content',
      from: { kind: 'local' },
    });
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  it('lone existing local directory positional lowers to structure (ls)', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ json: true }, ['src']); // packages/octocode/src exists at test cwd
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'structure',
      from: { kind: 'local' },
    });
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  it('--symbols on a local directory uses the shared outline flow', async () => {
    await run({ symbols: true, ext: 'ts', limit: '3' }, ['src']);
    expect(outlineSymbols).toHaveBeenCalledWith(
      'src',
      expect.objectContaining({ symbols: true, ext: 'ts', limit: '3' })
    );
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('--repo --symbols materializes then outlines the saved local path', async () => {
    materializeRemoteForCli.mockResolvedValue({
      localPath: '/tmp/octocode-react/ReactHooks.js',
      repoRoot: '/tmp/octocode-react',
    });
    await run(
      { repo: 'facebook/react', symbols: true, branch: 'main', json: true },
      ['packages/react/src/ReactHooks.js']
    );
    expect(materializeRemoteForCli).toHaveBeenCalledWith({
      repoRef: 'facebook/react',
      path: 'packages/react/src/ReactHooks.js',
      branch: 'main',
      forceRefresh: undefined,
      kind: 'file',
    });
    expect(outlineSymbols).toHaveBeenCalledWith(
      '/tmp/octocode-react/ReactHooks.js',
      expect.objectContaining({ symbols: true, json: true }),
      expect.objectContaining({
        structured: expect.objectContaining({
          location: { localPath: '/tmp/octocode-react/ReactHooks.js' },
        }),
      })
    );
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('lone owner/repo positional lowers to remote structure', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ json: true }, ['facebook/react']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'structure',
      from: { kind: 'github', repo: 'facebook/react' },
    });
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  it('lone owner/repo/file positional lowers to content (remote cat)', async () => {
    runOqlSearch.mockResolvedValue(contentEnvelope());
    await run({ json: true }, ['facebook/react/packages/react/src/index.js']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'content',
      from: { kind: 'github', repo: 'facebook/react' },
    });
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  it('lone owner/repo/dir positional with no extension lowers to remote structure', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ json: true }, ['facebook/react/packages/react']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'structure',
      from: { kind: 'github', repo: 'facebook/react' },
    });
  });

  it('text plus existing local file positional stays a scoped code search', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ json: true }, ['octocode', 'package.json']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'code',
      from: { kind: 'local' },
      where: { kind: 'text', value: 'octocode' },
    });
  });

  it('two local file positionals with --target diff build a local direct-file diff', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ json: true, target: 'diff' }, [
      'package.json',
      'tsconfig.json',
    ]);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    // Bug 4: BOTH the base (from.path) and the head (params.path) must be
    // ABSOLUTE paths — a relative basename fails the localGetFileContent
    // allowed-directories guard ("Path 'tsconfig.json' is outside allowed
    // directories").
    const from = (input as { from?: { kind?: string; path?: string } }).from;
    const params = (input as { params?: { path?: string } }).params;
    expect(input).toMatchObject({ schema: 'oql', target: 'diff' });
    expect(from?.kind).toBe('local');
    expect(path.isAbsolute(from?.path ?? '')).toBe(true);
    expect(from?.path).toContain('package.json');
    expect(path.isAbsolute(params?.path ?? '')).toBe(true);
    expect(params?.path).toContain('tsconfig.json');
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  // Bug 3: exit-code classification via search must reach 3/4/7, not collapse
  // every diagnostic-bearing result to USAGE (2).
  function diagnosticEnvelope(code: string, message: string) {
    return {
      results: [],
      diagnostics: [
        {
          code,
          severity: 'error',
          message,
          blocksAnswer: true,
        },
      ],
      provenance: [],
      evidence: { answerReady: false, complete: false, kind: 'empty' },
    };
  }

  it('returns NOT_FOUND (3) when a query resolves nothing (404)', async () => {
    runOqlSearch.mockResolvedValue(
      diagnosticEnvelope(
        'invalidQuery',
        'Repository nope/missing not found (404).'
      )
    );
    await run({
      query:
        '{"target":"code","from":{"kind":"github","repo":"nope/missing"},"where":{"kind":"text","value":"x"}}',
    });
    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
  });

  it('returns AUTH (4) when a query fails with bad credentials', async () => {
    runOqlSearch.mockResolvedValue(
      diagnosticEnvelope('invalidQuery', 'Bad credentials (401).')
    );
    await run({
      query:
        '{"target":"code","from":{"kind":"github","repo":"nope/missing"},"where":{"kind":"text","value":"x"}}',
    });
    expect(process.exitCode).toBe(EXIT.AUTH);
  });

  it('returns RATE_LIMIT (7) when a query is rate limited', async () => {
    runOqlSearch.mockResolvedValue(
      diagnosticEnvelope('rateLimited', 'API rate limit exceeded (429).')
    );
    await run({
      query:
        '{"target":"code","from":{"kind":"github","repo":"nope/missing"},"where":{"kind":"text","value":"x"}}',
    });
    expect(process.exitCode).toBe(EXIT.RATE_LIMIT);
  });

  it('returns USAGE (2) for a genuinely malformed query', async () => {
    runOqlSearch.mockResolvedValue(
      diagnosticEnvelope(
        'invalidQuery',
        'target:"diff" needs either {prNumber} or {baseRef,headRef,path}.'
      )
    );
    await run({
      query: '{"target":"diff","from":{"kind":"github","repo":"a/b"}}',
    });
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  it('owner/repo#N with --target diff lowers to a PR patch diff', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ json: true, target: 'diff' }, ['facebook/react#123']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'diff',
      from: { kind: 'github', repo: 'facebook/react' },
      params: { prNumber: 123 },
    });
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  it('tree shorthand keeps structure filters on fetch.tree', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        tree: true,
        name: 's*.ts',
        ext: 'ts,tsx',
        'files-only': true,
        sort: 'name',
        'sort-reverse': true,
        limit: '5',
      },
      ['src']
    );
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'structure',
      fetch: {
        tree: {
          pattern: 's*.ts',
          extensions: ['ts', 'tsx'],
          filesOnly: true,
          sortBy: 'name',
          reverse: true,
        },
      },
      limit: 5,
    });
    expect((input as { controls?: unknown }).controls).toBeUndefined();
    expect((input as { where?: unknown }).where).toBeUndefined();
  });

  it('file-discovery shorthand lowers to the files target', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        search: 'path',
        ext: 'ts',
        name: '*auth*',
        entry: 'file',
        'min-depth': '1',
        'modified-within': '7d',
      },
      ['auth', '.']
    );
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      target: 'files',
      scope: { include: ['**/*.ts'], minDepth: 1 },
      where: {
        kind: 'all',
        of: expect.arrayContaining([
          { kind: 'field', field: 'entryType', op: '=', value: 'file' },
          { kind: 'field', field: 'extension', op: '=', value: 'ts' },
          { kind: 'field', field: 'basename', op: 'glob', value: '*auth*' },
          {
            kind: 'field',
            field: 'modified',
            op: 'within',
            value: '7d',
          },
        ]),
      },
    });
  });

  it('search both builds an independent files/code OQL batch', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ search: 'both', ext: 'ts', limit: '20' }, ['auth', '.']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      combine: 'independent',
      limit: 20,
      queries: [
        { target: 'files', scope: { include: ['**/*.ts'] } },
        { target: 'code', scope: { include: ['**/*.ts'] } },
      ],
    });
  });

  it('--pattern + --lang builds a structural sugar object; positional is target', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ pattern: 'eval($X)', lang: 'ts' }, ['./src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'code',
      where: { kind: 'structural', lang: 'ts', pattern: 'eval($X)' },
    });
    expect(input.from).toMatchObject({ kind: 'local' });
  });

  it('--rule accepts a grep-compatible YAML structural rule string', async () => {
    const yamlRule = ['rule:', '  pattern: "eval($X)"'].join('\n');
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ rule: yamlRule, lang: 'ts' }, ['./src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'code',
      where: { kind: 'structural', lang: 'ts', rule: yamlRule },
    });
    expect(input.from).toMatchObject({ kind: 'local' });
  });

  it('--rule preserves JSON structural rule object shorthand', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ rule: '{"pattern":"eval($X)"}', lang: 'ts' }, ['./src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'code',
      where: {
        kind: 'structural',
        lang: 'ts',
        rule: { pattern: 'eval($X)' },
      },
    });
  });

  it('--pattern without --lang is a usage error', async () => {
    await run({ pattern: 'eval($X)' }, ['./src']);
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('--regex --pcre2 builds canonical regex where with dialect', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ regex: 'a(?=b)', pcre2: true }, ['./src']);
    const [input] = runOqlSearch.mock.calls[0]!;
    expect((input as { where?: unknown }).where).toMatchObject({
      kind: 'regex',
      value: 'a(?=b)',
      dialect: 'pcre2',
    });
  });

  it('owner/repo target becomes a github sugar source', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ lang: 'tsx' }, ['useEffect', 'facebook/react']);
    const [input] = runOqlSearch.mock.calls[0]!;
    expect(input).toMatchObject({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { language: 'tsx' },
      where: { kind: 'text', value: 'useEffect' },
    });
  });

  it('--repo preserves repo-relative path for code search', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ repo: 'facebook/react' }, ['useEffect', 'packages/react/src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react/src' },
      where: { kind: 'text', value: 'useEffect' },
    });
  });

  it('--path scopes local search roots for file discovery', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ source: 'local', path: './src', search: 'path', ext: 'ts' }, [
      'hooks',
    ]);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'files',
      from: {
        kind: 'local',
        path: expect.stringContaining('/src'),
      },
      scope: { include: ['**/*.ts'] },
      where: {
        kind: 'all',
        of: [
          { kind: 'field', field: 'extension', op: '=', value: 'ts' },
          { kind: 'field', field: 'basename', op: 'glob', value: '*hooks*' },
        ],
      },
    });
  });

  it('--repo plus --path scopes remote file discovery', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      { repo: 'facebook/react', path: 'packages/react', search: 'both' },
      ['useState']
    );
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      schema: 'oql',
      combine: 'independent',
      queries: [
        {
          target: 'files',
          from: { kind: 'github', repo: 'facebook/react' },
          scope: { path: 'packages/react' },
        },
        {
          target: 'code',
          from: { kind: 'github', repo: 'facebook/react' },
          scope: { path: 'packages/react' },
        },
      ],
    });
  });

  it('--entry f/d aliases lower to canonical file and directory predicates', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ search: 'path', entry: 'f' }, ['parser', 'src']);
    let [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'files',
      where: {
        kind: 'all',
        of: [
          { kind: 'field', field: 'entryType', op: '=', value: 'file' },
          { kind: 'field', field: 'basename', op: 'glob', value: '*parser*' },
        ],
      },
    });

    runOqlSearch.mockClear();
    await run({ search: 'path', entry: 'd' }, ['parser', 'src']);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'files',
      where: {
        kind: 'all',
        of: [
          { kind: 'field', field: 'entryType', op: '=', value: 'directory' },
          { kind: 'field', field: 'basename', op: 'glob', value: '*parser*' },
        ],
      },
    });
  });

  it('--repo preserves repo-relative path for content reads', async () => {
    runOqlSearch.mockResolvedValue(contentEnvelope());
    await run({ repo: 'facebook/react', 'content-view': 'none' }, [
      'README.md',
    ]);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'content',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'README.md' },
      fetch: { content: { contentView: 'none' } },
    });
  });

  it('--content-view lowers to canonical fetch.content.contentView', async () => {
    runOqlSearch.mockResolvedValue(contentEnvelope());
    await run({ 'content-view': 'none' }, ['./a.ts']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'content',
      fetch: { content: { contentView: 'none' } },
    });
  });

  it('--repo preserves repo-relative path for structure and semantics', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ repo: 'facebook/react', tree: true, entry: 'directory' }, [
      'packages/react',
    ]);
    let [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'structure',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      fetch: { tree: { directoriesOnly: true } },
    });

    runOqlSearch.mockClear();
    await run(
      {
        repo: 'facebook/react',
        op: 'references',
        symbol: 'useState',
        line: '42',
      },
      ['packages/react/src/ReactHooks.js']
    );
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'semantics',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react/src/ReactHooks.js' },
      params: { type: 'references', symbolName: 'useState', lineHint: 42 },
    });
  });

  it('repository, package, PR, commit, and file flags lower into params', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        target: 'repositories',
        lang: 'TypeScript',
        forks: '>10',
        'good-first-issues': '>0',
        license: 'mit',
        created: '>2024-01-01',
        match: 'name,readme',
        sort: 'stars',
        concise: true,
      },
      ['mcp server']
    );
    let [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'repositories',
      params: {
        language: 'TypeScript',
        forks: '>10',
        goodFirstIssues: '>0',
        license: 'mit',
        created: '>2024-01-01',
        match: ['name', 'readme'],
        sort: 'stars',
        concise: true,
      },
    });
    expect(input.controls).toBeUndefined();

    runOqlSearch.mockClear();
    await run({ target: 'packages' }, ['zod']);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'packages',
      params: { packageName: 'zod' },
    });

    runOqlSearch.mockClear();
    await run(
      {
        target: 'pullRequests',
        base: 'main',
        head: 'feature',
        sort: 'updated',
        order: 'asc',
        draft: true,
        patches: true,
        comments: true,
      },
      ['facebook/react#123']
    );
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        prNumber: 123,
        base: 'main',
        head: 'feature',
        sort: 'updated',
        order: 'asc',
        draft: true,
        content: {
          metadata: true,
          changedFiles: true,
          patches: { mode: 'all' },
          comments: { discussion: true, reviewInline: true },
        },
      },
    });
    expect(input.controls).toBeUndefined();

    runOqlSearch.mockClear();
    await run({ patches: true, comments: true, commits: true }, [
      'facebook/react#123',
    ]);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        prNumber: 123,
        content: {
          metadata: true,
          changedFiles: true,
          patches: { mode: 'all' },
          comments: { discussion: true, reviewInline: true },
          commits: { list: true },
        },
      },
    });

    runOqlSearch.mockClear();
    await run({ patches: true }, [
      'https://github.com/facebook/react/pull/123',
    ]);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        prNumber: 123,
        content: {
          metadata: true,
          changedFiles: true,
          patches: { mode: 'all' },
        },
      },
    });

    runOqlSearch.mockClear();
    await run({ pr: '123', comments: true }, ['facebook/react']);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        prNumber: 123,
        content: {
          metadata: true,
          comments: { discussion: true, reviewInline: true },
        },
      },
    });

    runOqlSearch.mockClear();
    await run({ target: 'pullRequests', query: 'fix auth' }, [
      'facebook/react',
    ]);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: { keywordsToSearch: ['fix auth'] },
    });

    runOqlSearch.mockClear();
    await run({ file: 'packages/react/src/index.js' }, ['facebook/react#123']);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        prNumber: 123,
        content: {
          metadata: true,
          changedFiles: true,
          patches: { mode: 'selected', files: ['packages/react/src/index.js'] },
        },
      },
    });

    runOqlSearch.mockClear();
    await run({ target: 'commits', author: 'octocat', patches: true }, [
      'facebook/react/packages/react/src',
    ]);
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'commits',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react/src' },
      params: { author: 'octocat', includeDiff: true },
    });

    runOqlSearch.mockClear();
    await run(
      {
        target: 'files',
        empty: true,
        'min-depth': '1',
        'modified-before': '30d',
        executable: true,
        'exclude-dir': 'node_modules,dist',
      },
      ['.']
    );
    [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'files',
      scope: { minDepth: 1, excludeDir: ['node_modules', 'dist'] },
      where: {
        kind: 'all',
        of: expect.arrayContaining([
          { kind: 'field', field: 'empty', op: '=', value: true },
          {
            kind: 'field',
            field: 'modified',
            op: 'before',
            value: '30d',
          },
          { kind: 'field', field: 'executable', op: '=', value: true },
        ]),
      },
    });
  });

  it('--target/--op/page flags lower into canonical semantics OQL', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        target: 'semantics',
        op: 'references',
        symbol: 'runCLI',
        line: '12',
        page: '2',
        'items-per-page': '5',
      },
      ['./src/index.ts']
    );
    const [input] = runOqlSearch.mock.calls[0]!;
    expect(input).toMatchObject({
      target: 'semantics',
      params: {
        type: 'references',
        symbolName: 'runCLI',
        lineHint: 12,
      },
      page: 2,
      itemsPerPage: 5,
    });
  });

  it('--items-per-page lowers into canonical OQL pagination', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        target: 'research',
        intent: 'reachability',
        'items-per-page': '1',
      },
      ['./src']
    );
    const [input] = runOqlSearch.mock.calls[0]!;
    expect(input).toMatchObject({
      target: 'research',
      itemsPerPage: 1,
      params: {
        intent: 'reachability',
      },
    });
  });

  it('--symbols --kind uses the shared outline flow', async () => {
    await run({ symbols: true, kind: 'function' }, ['./src/index.ts']);
    expect(outlineSymbols).toHaveBeenCalledWith(
      './src/index.ts',
      expect.objectContaining({ symbols: true, kind: 'function' })
    );
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('two GitHub file refs with --target diff lower to a direct GitHub file diff', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ target: 'diff' }, [
      'facebook/react/packages/react/src/index.js',
      'facebook/react@main/packages/react/src/index.js',
    ]);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input).toMatchObject({
      target: 'diff',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        baseRef: '',
        headRef: 'main',
        path: 'packages/react/src/index.js',
      },
    });
  });

  it('search controls lower into OQL controls', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run(
      {
        'context-lines': '2',
        'invert-match': true,
        'only-matching': true,
        'max-matches': '3',
      },
      ['TODO', './src']
    );
    const [input] = runOqlSearch.mock.calls[0]!;
    expect(input).toMatchObject({
      controls: {
        search: {
          contextLines: 2,
          invertMatch: true,
          onlyMatching: true,
          maxMatchesPerFile: 3,
        },
      },
    });
  });

  it('--raw renders content rows only', async () => {
    runOqlSearch.mockResolvedValue(contentEnvelope());
    await run({ target: 'content', raw: true, 'content-view': 'none' }, [
      './a.ts',
    ]);
    expect(stdout).toBe('export const value = 1;\n');
  });

  it('rejects ambiguous predicate flags', async () => {
    await run({ pattern: 'eval($X)', regex: 'eval' }, ['./src']);
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('rejects invalid --content-view values', async () => {
    await run({ 'content-view': 'bogus' }, ['./a.ts']);
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });

  it('rejects --raw with --json', async () => {
    await run({ raw: true, json: true }, ['x']);
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(runOqlSearch).not.toHaveBeenCalled();
  });
});
