import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const runOqlSearch = vi.fn();
const oqlSchemaText = vi.fn(() => '{"schema":"oql/v1"}');

vi.mock('@octocodeai/octocode-tools-core/oql', async () => {
  // keep the real shorthand lowering (tools-core owns it); mock only execution
  const actual = await vi.importActual<
    typeof import('@octocodeai/octocode-tools-core/oql')
  >('@octocodeai/octocode-tools-core/oql');
  return {
    ...actual,
    runOqlSearch: (...args: unknown[]) => runOqlSearch(...args),
    oqlSchemaText: () => oqlSchemaText(),
  };
});

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

describe('octocode search command', () => {
  it('--scheme prints the OQL schema without running a query', async () => {
    await run({ scheme: true });
    expect(stdout).toContain('oql/v1');
    expect(runOqlSearch).not.toHaveBeenCalled();
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
});

describe('octocode search shorthand sugar', () => {
  it('positional text builds a code/text sugar object', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({}, ['runCLI', './src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input.text).toBe('runCLI');
    expect(String(input.path)).toMatch(/src$/);
  });

  it('--pattern + --lang builds a structural sugar object; positional is target', async () => {
    runOqlSearch.mockResolvedValue(proofEnvelope());
    await run({ pattern: 'eval($X)', lang: 'ts' }, ['./src']);
    const [input] = runOqlSearch.mock.calls[0]! as [Record<string, unknown>];
    expect(input.pattern).toBe('eval($X)');
    expect(input.lang).toBe('ts');
    expect(String(input.path)).toMatch(/src$/);
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
    await run({ type: 'tsx' }, ['useEffect', 'facebook/react']);
    const [input] = runOqlSearch.mock.calls[0]!;
    expect(input).toMatchObject({
      text: 'useEffect',
      repo: 'facebook/react',
      langType: 'tsx',
    });
  });
});
