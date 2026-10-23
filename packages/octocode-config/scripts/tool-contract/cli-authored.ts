// The authored part of the CLI flags: what a tool's schema cannot say. Which
// short letters a familiar command (rg, grep, fd, ast-grep, gh) already uses
// for the same meaning, which fields read as positional arguments, and
// flag-form examples per tool. cli-flags.ts checks every name here against
// the schema, so generation fails when one names a field or value it lacks.

/**
 * One positional argument. `field` names what it fills: one field, two
 * fields split at `separator`, or per operation. `lines` reads a
 * `PATH:N` / `PATH:N-M` suffix into that field. A positional the caller
 * already set by flag is skipped, so `-e A PATH` fills `path`.
 */
export interface CliPositional {
  name: string;
  field: string | [string, string] | Record<string, string | [string, string]>;
  separator?: string;
  default?: string;
  variadic?: boolean;
  lines?: { field: string; kind: 'ranges' | 'line' };
  /** Skipped when this operation's call sets `field` by flag and fewer arguments remain than positionals. */
  omitWhen?: { operation: string; field: string };
}

export interface Authored {
  positionals: CliPositional[];
  /** Short letter → field (a flag of its own) or field=value (a preset). */
  shorts?: Record<string, string>;
  ownerRepo?: boolean;
  examples: string[];
}

const PATH = (fallback?: string): CliPositional => ({
  name: 'PATH',
  field: 'path',
  ...(fallback !== undefined && { default: fallback }),
});

/** gh: `-L/--limit` is the page size. */
const GH_LIMIT = { L: 'pageSize' };

export const AUTHORED: Record<string, Authored> = {
  localSearch: {
    positionals: [{ name: 'PATTERN', field: 'matchString' }, PATH('.')],
    // rg letters; `-E` is grep's (rg uses it for --encoding, which Octocode lacks).
    shorts: {
      e: 'matchString',
      F: 'regex=literal',
      E: 'regex=rust',
      P: 'regex=pcre2',
      i: 'caseMode=insensitive',
      S: 'caseMode=smart',
      s: 'caseMode=sensitive',
      w: 'wholeWord',
      v: 'invertMatch',
      U: 'multiline=on',
      g: 'include',
      t: 'language',
      d: 'maxDepth',
      A: 'contextAfter',
      B: 'contextBefore',
      C: 'contextLines',
      l: 'resultView=files',
      c: 'resultView=countLines',
      o: 'resultView=matchOnly',
    },
    examples: ["octocode localSearch registerHandler src -w -g '*.ts' -C 2"],
  },
  structureSearch: {
    positionals: [PATH('.')],
    // fd letters.
    shorts: {
      g: 'include',
      e: 'extensions',
      t: 'entryType',
      d: 'maxDepth',
      E: 'exclude',
      H: 'includeHidden',
      I: 'includeIgnored',
    },
    examples: ['octocode structureSearch files src -e ts -d 2'],
  },
  astSearch: {
    positionals: [PATH('.')],
    // ast-grep letters.
    shorts: { p: 'pattern', l: 'language' },
    examples: [
      'octocode astSearch symbols src/server.ts --symbolName createServer --kinds function',
      "octocode astSearch match src -l typescript -p 'function $F($$$A) { $$$B }'",
    ],
  },
  localFetch: {
    positionals: [
      { name: 'PATH[:LINES]', field: 'path', lines: { field: 'ranges', kind: 'ranges' } },
    ],
    // grep letters.
    shorts: { e: 'matchString', C: 'contextLines' },
    examples: ['octocode localFetch src/index.ts:1-40'],
  },
  lspSearch: {
    positionals: [
      {
        name: 'PATH[:LINE]',
        field: 'path',
        lines: { field: 'line', kind: 'line' },
        omitWhen: { operation: 'workspaceSymbol', field: 'workspaceRoot' },
      },
      { name: 'SYMBOL', field: 'symbolName' },
    ],
    // Call chains: callers walks up from a function, callees down.
    examples: [
      'octocode lspSearch references src/index.ts:10 run',
      'octocode lspSearch callers src/index.ts:10 run',
    ],
  },
  ghSearchRepo: {
    positionals: [{ name: 'KEYWORD', field: 'keywords', variadic: true }],
    shorts: GH_LIMIT,
    examples: ['octocode ghSearchRepo ripgrep --sort stars:desc -L 3'],
  },
  ghSearchCode: {
    positionals: [{ name: 'KEYWORD', field: 'keywords', variadic: true }],
    shorts: GH_LIMIT,
    ownerRepo: true,
    examples: ['octocode ghSearchCode registerTool -R bgauryy/octocode --extensions ts'],
  },
  ghStructure: {
    positionals: [PATH()],
    shorts: GH_LIMIT,
    ownerRepo: true,
    examples: ['octocode ghStructure tree crates -R BurntSushi/ripgrep'],
  },
  ghGetFileContent: {
    positionals: [
      { name: 'PATH[:LINES]', field: 'path', lines: { field: 'ranges', kind: 'ranges' } },
    ],
    // grep letters, as localFetch.
    shorts: { e: 'matchString', C: 'contextLines' },
    ownerRepo: true,
    examples: ['octocode ghGetFileContent crates/searcher/src/lib.rs:1-40 -R BurntSushi/ripgrep'],
  },
  ghSearchHistory: {
    positionals: [{ name: 'KEYWORD', field: 'keywords', variadic: true }],
    shorts: { ...GH_LIMIT, s: 'state' },
    ownerRepo: true,
    examples: ['octocode ghSearchHistory pullRequest fix -R BurntSushi/ripgrep -s merged -L 3'],
  },
  ghGetHistoryItem: {
    positionals: [
      {
        name: 'NUMBER|REF|BASE...HEAD',
        field: {
          pullRequest: 'number',
          issue: 'number',
          commit: 'ref',
          compare: ['base', 'head'],
        },
        separator: '...',
      },
    ],
    ownerRepo: true,
    examples: ['octocode ghGetHistoryItem pullRequest 3502 -R BurntSushi/ripgrep --sections body'],
  },
  artifactSearch: {
    positionals: [{ name: 'PACKAGE', field: 'packageName' }],
    shorts: GH_LIMIT,
    examples: ['octocode artifactSearch zod --ecosystem npm'],
  },
  ghCloneRepo: {
    positionals: [{ name: 'OWNER/REPO', field: ['owner', 'repo'], separator: '/' }],
    examples: ['octocode ghCloneRepo vercel/next.js --path packages/next'],
  },
  // JSON only: its questions are nested objects; help lists each form's example.
  clasify: { positionals: [], examples: [] },
};
