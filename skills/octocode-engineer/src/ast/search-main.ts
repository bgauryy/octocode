#!/usr/bin/env node


import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

import { ALLOWED_EXTS, isPythonFile } from '../types/index.js';

import type { NapiConfig, SgNode, SgRoot } from '@ast-grep/napi';

const nodeRequire = createRequire(import.meta.url);
const astGrep = nodeRequire('@ast-grep/napi') as typeof import('@ast-grep/napi');
const {
  js: astJs,
  parse: astParse,
  registerDynamicLanguage,
  ts: astTs,
  tsx: astTsx,
} = astGrep;

let pythonRegistered = false;

async function ensurePythonRegistered(): Promise<boolean> {
  if (pythonRegistered) return true;
  try {
    const langMod = await import('@ast-grep/lang-python');
    const config = langMod.default;
    registerDynamicLanguage({ python: config });
    pythonRegistered = true;
    return true;
  } catch {
    return false;
  }
}

export interface AstSearchOptions {
  root: string;
  pattern: string | null;
  kind: string | null;
  preset: string | null;
  rule: NapiConfig | null;
  json: boolean;
  limit: number;
  includeTests: boolean;
  ignoreDirs: Set<string>;
  context: number;
}

export interface AstMatch {
  file: string;
  kind: string;
  text: string;
  lineStart: number;
  lineEnd: number;
  columnStart: number;
  columnEnd: number;
  metaVariables?: Record<string, string>;
}

export interface AstSearchResult {
  query: string;
  queryType: 'pattern' | 'kind' | 'preset' | 'rule';
  totalMatches: number;
  totalFiles: number;
  matches: AstMatch[];
  
  _sourceByFile?: Map<string, string[]>;
}

type PresetRule = NapiConfig & { description: string };

export const PRESETS: Record<string, PresetRule> = {
  'empty-catch': {
    rule: {
      kind: 'catch_clause',
      has: {
        kind: 'statement_block',
        regex: '^\\{\\s*\\}$',
      },
    },
    description: 'Empty catch blocks that silently swallow errors',
  },
  'console-log': {
    rule: {
      pattern: 'console.log($$$ARGS)',
    },
    description: 'console.log calls left in production code',
  },
  'console-any': {
    rule: {
      pattern: 'console.$METHOD($$$ARGS)',
    },
    description: 'Any console method call (log, warn, error, debug, etc.)',
  },
  debugger: {
    rule: {
      kind: 'debugger_statement',
    },
    description: 'Debugger statements left in code',
  },
  'todo-fixme': {
    rule: {
      kind: 'comment',
      regex: '(?i)(TODO|FIXME|HACK|XXX|BUG)',
    },
    description: 'TODO, FIXME, HACK, XXX, BUG comments',
  },
  'any-type': {
    rule: {
      kind: 'predefined_type',
      regex: '^any$',
    },
    description: 'Explicit `any` type annotations',
  },
  'type-assertion': {
    rule: {
      kind: 'as_expression',
    },
    description: 'TypeScript type assertions (as X)',
  },
  'non-null-assertion': {
    rule: {
      kind: 'non_null_expression',
    },
    description: 'Non-null assertions (x!)',
  },
  'fat-arrow-body': {
    rule: {
      kind: 'arrow_function',
      has: {
        kind: 'statement_block',
      },
    },
    description:
      'Arrow functions with statement block bodies (could be expression)',
  },
  'nested-ternary': {
    rule: {
      kind: 'ternary_expression',
      has: {
        kind: 'ternary_expression',
        stopBy: 'end',
      },
    },
    description: 'Nested ternary expressions (hard to read)',
  },
  'throw-string': {
    rule: {
      kind: 'throw_statement',
      has: {
        kind: 'string',
      },
    },
    description: 'Throwing string literals instead of Error objects',
  },
  'switch-no-default': {
    rule: {
      kind: 'switch_statement',
      not: {
        has: {
          kind: 'switch_default',
          stopBy: 'end',
        },
      },
    },
    description: 'Switch statements without a default case',
  },
  'class-declaration': {
    rule: {
      kind: 'class_declaration',
    },
    description: 'All class declarations',
  },
  'async-function': {
    rule: {
      kind: 'function_declaration',
      regex: '^async ',
    },
    description: 'Async function declarations',
  },
  'export-default': {
    rule: {
      kind: 'export_statement',
      has: {
        field: 'default',
      },
    },
    description: 'Default exports',
  },
  'import-star': {
    rule: {
      kind: 'import_statement',
      has: {
        kind: 'namespace_import',
      },
    },
    description: 'Namespace imports (import * as X)',
  },
  'catch-rethrow': {
    rule: {
      kind: 'catch_clause',
      has: {
        kind: 'statement_block',
        has: {
          kind: 'throw_statement',
        },
      },
    },
    description: 'Catch blocks that only re-throw the caught error',
  },
  'promise-all': {
    rule: {
      pattern: 'Promise.all($$$ARGS)',
    },
    description: 'Promise.all calls (check for missing error handling)',
  },
  'boolean-param': {
    rule: {
      kind: 'type_annotation',
      has: {
        kind: 'predefined_type',
        regex: '^boolean$',
      },
    },
    description: 'Function parameters typed as boolean',
  },
  'magic-number': {
    rule: {
      kind: 'number',
      not: {
        regex: '^[01]$',
      },
    },
    description: 'Numeric literals (excluding 0 and 1) — potential magic numbers',
  },
  'deep-callback': {
    rule: {
      kind: 'arrow_function',
      inside: {
        kind: 'arrow_function',
        inside: {
          kind: 'arrow_function',
          stopBy: 'end',
        },
        stopBy: 'end',
      },
    },
    description: 'Deeply nested arrow function callbacks (3+ levels)',
  },
  'unused-var': {
    rule: {
      kind: 'variable_declarator',
      not: {
        has: {
          kind: 'call_expression',
          stopBy: 'end',
        },
      },
    },
    description: 'Variable declarations without call expressions (candidates for dead code)',
  },

  'py-bare-except': {
    rule: {
      kind: 'except_clause',
      not: {
        has: { kind: 'identifier' },
      },
    },
    description: '[Python] Bare except: clause with no exception type',
  },
  'py-pass-except': {
    rule: {
      kind: 'except_clause',
      has: {
        kind: 'block',
        has: { kind: 'pass_statement' },
      },
    },
    description: '[Python] except: pass — silently swallowed exception',
  },
  'py-broad-except': {
    rule: {
      kind: 'except_clause',
      has: {
        kind: 'identifier',
        regex: '^(Exception|BaseException)$',
      },
    },
    description: '[Python] Overly broad except (Exception/BaseException)',
  },
  'py-global-stmt': {
    rule: {
      kind: 'global_statement',
    },
    description: '[Python] Global variable mutation',
  },
  'py-exec-call': {
    rule: {
      kind: 'call',
      has: { kind: 'identifier', regex: '^exec$' },
    },
    description: '[Python] exec() — dynamic code execution',
  },
  'py-eval-call': {
    rule: {
      kind: 'call',
      has: { kind: 'identifier', regex: '^eval$' },
    },
    description: '[Python] eval() — dynamic evaluation',
  },
  'py-star-import': {
    rule: {
      kind: 'import_from_statement',
      has: { kind: 'wildcard_import' },
    },
    description: '[Python] from X import * — wildcard import',
  },
  'py-assert': {
    rule: {
      kind: 'assert_statement',
    },
    description: '[Python] assert statement (stripped with -O flag)',
  },
  'py-mutable-default': {
    rule: {
      kind: 'default_parameter',
      any: [
        { has: { kind: 'list' } },
        { has: { kind: 'dictionary' } },
        { has: { kind: 'set' } },
      ],
    },
    description: '[Python] Mutable default argument (list/dict/set literal)',
  },
  'py-todo-fixme': {
    rule: {
      kind: 'comment',
      regex: '(?i)(TODO|FIXME|HACK|XXX|BUG)',
    },
    description: '[Python] TODO, FIXME, HACK, XXX, BUG comments',
  },
  'py-print-call': {
    rule: {
      kind: 'call',
      has: { kind: 'identifier', regex: '^print$' },
    },
    description: '[Python] print() calls in production code',
  },
  'py-class': {
    rule: {
      kind: 'class_definition',
    },
    description: '[Python] All class definitions',
  },
  'py-async-function': {
    rule: {
      kind: 'function_definition',
      regex: '^async ',
    },
    description: '[Python] Async function definitions',
  },
};

function isTestFile(filePath: string): boolean {
  const base = path.basename(filePath);
  return (
    /\.(test|spec)\.(ts|tsx|js|jsx|mjs|cjs)$/.test(base) ||
    base.startsWith('test_') ||
    /^test_.*\.py$/.test(base) ||
    /_test\.py$/.test(base) ||
    filePath.includes('__tests__') ||
    filePath.includes('/tests/')
  );
}

export function collectSearchFiles(
  root: string,
  opts: Pick<AstSearchOptions, 'includeTests' | 'ignoreDirs'>
): string[] {
  const files: string[] = [];
  const walk = (dir: string): void => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      if (opts.ignoreDirs.has(entry.name)) continue;
      if (entry.isSymbolicLink()) continue;
      const next = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(next);
        continue;
      }
      if (!entry.isFile()) continue;
      if (entry.name.endsWith('.d.ts')) continue;
      const ext = path.extname(entry.name);
      if (!ALLOWED_EXTS.has(ext)) continue;
      if (!opts.includeTests && isTestFile(next)) continue;
      files.push(next);
    }
  };
  walk(root);
  return files;
}

type AstParser = { parse(src: string): SgRoot };

function parserForExt(ext: string): AstParser | 'python' {
  switch (ext) {
    case '.py':
      return 'python';
    case '.tsx':
      return astTsx;
    case '.jsx':
      return astTsx;
    case '.js':
    case '.mjs':
    case '.cjs':
      return astJs;
    case '.ts':
    default:
      return astTs;
  }
}

function extractMetaVars(
  node: SgNode,
  pattern: string
): Record<string, string> {
  const vars: Record<string, string> = {};
  let match: RegExpExecArray | null;

  const triplePattern = /\$\$\$([A-Z_][A-Z0-9_]*)/g;
  const triplNames = new Set<string>();
  while ((match = triplePattern.exec(pattern)) !== null) {
    const name = match[1];
    triplNames.add(name);
    const multiMatch = node.getMultipleMatches(name);
    if (multiMatch.length > 0) {
      vars[`$$$${name}`] = multiMatch.map(n => n.text()).join(', ');
    }
  }

  const singlePattern = /(?<!\$)\$([A-Z_][A-Z0-9_]*)(?!\$)/g;
  while ((match = singlePattern.exec(pattern)) !== null) {
    const name = match[1];
    if (triplNames.has(name)) continue;
    const matchNode = node.getMatch(name);
    if (matchNode) vars[`$${name}`] = matchNode.text();
  }

  return vars;
}

function nodeToMatch(
  node: SgNode,
  file: string,
  pattern: string | null
): AstMatch {
  const range = node.range();
  const result: AstMatch = {
    file,
    kind: String(node.kind()),
    text: node.text(),
    lineStart: range.start.line + 1,
    lineEnd: range.end.line + 1,
    columnStart: range.start.column,
    columnEnd: range.end.column,
  };
  if (pattern) {
    const vars = extractMetaVars(node, pattern);
    if (Object.keys(vars).length > 0) result.metaVariables = vars;
  }
  return result;
}

export function searchFile(
  filePath: string,
  source: string,
  matcher: string | number | NapiConfig,
  patternStr: string | null,
  limit: number
): AstMatch[] {
  const ext = path.extname(filePath);
  const parser = parserForExt(ext);
  let nodes: SgNode[];
  try {
    const root =
      parser === 'python'
        ? astParse('python', source).root()
        : parser.parse(source).root();
    nodes = root.findAll(matcher);
  } catch {
    return [];
  }
  const matches: AstMatch[] = [];
  for (const node of nodes) {
    if (matches.length >= limit) break;
    matches.push(nodeToMatch(node, filePath, patternStr));
  }
  return matches;
}

export function runSearch(
  files: string[],
  opts: AstSearchOptions,
  root: string
): AstSearchResult {
  let matcher: string | NapiConfig;
  let queryLabel: string;
  let queryType: AstSearchResult['queryType'];
  let patternStr: string | null = null;

  if (opts.preset) {
    const preset = PRESETS[opts.preset];
    if (!preset) {
      const available = Object.keys(PRESETS).join(', ');
      throw new Error(
        `Unknown preset: "${opts.preset}". Available: ${available}`
      );
    }
    matcher = preset;
    queryLabel = `preset:${opts.preset} — ${preset.description}`;
    queryType = 'preset';
  } else if (opts.rule) {
    matcher = opts.rule;
    queryLabel = `rule:${JSON.stringify(opts.rule)}`;
    queryType = 'rule';
  } else if (opts.kind) {
    matcher = { rule: { kind: opts.kind } } as NapiConfig;
    queryLabel = `kind:${opts.kind}`;
    queryType = 'kind';
  } else if (opts.pattern) {
    matcher = opts.pattern;
    patternStr = opts.pattern;
    queryLabel = `pattern:${opts.pattern}`;
    queryType = 'pattern';
  } else {
    throw new Error('Must provide --pattern, --kind, --preset, or --rule');
  }

  const allMatches: AstMatch[] = [];
  const filesWithMatches = new Set<string>();
  const sourceByFile =
    opts.context > 0 ? new Map<string, string[]>() : undefined;

  for (const filePath of files) {
    if (allMatches.length >= opts.limit) break;
    let source: string;
    try {
      source = fs.readFileSync(filePath, 'utf8');
    } catch {
      continue;
    }
    const relFile = path.relative(root, filePath);
    const remaining = opts.limit - allMatches.length;
    const fileMatches = searchFile(
      relFile,
      source,
      matcher,
      patternStr,
      remaining
    );
    if (fileMatches.length > 0) {
      filesWithMatches.add(relFile);
      allMatches.push(...fileMatches);
      if (sourceByFile) sourceByFile.set(relFile, source.split('\n'));
    }
  }

  const result: AstSearchResult = {
    query: queryLabel,
    queryType,
    totalMatches: allMatches.length,
    totalFiles: filesWithMatches.size,
    matches: allMatches,
  };
  if (sourceByFile) result._sourceByFile = sourceByFile;
  return result;
}

interface ParsedSearchArgs {
  opts: AstSearchOptions;
  listPresets: boolean;
}

export function parseSearchArgs(argv: string[]): ParsedSearchArgs {
  const opts: AstSearchOptions = {
    root: process.cwd(),
    pattern: null,
    kind: null,
    preset: null,
    rule: null,
    json: false,
    limit: 500,
    includeTests: false,
    ignoreDirs: new Set([
      '.git',
      '.next',
      '.yarn',
      '.cache',
      '.octocode',
      'node_modules',
      'dist',
      'coverage',
      'out',
    ]),
    context: 0,
  };
  let listPresets = false;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--pattern' || arg === '-p') {
      opts.pattern = argv[++i];
      continue;
    }
    if (arg.startsWith('--pattern=')) {
      opts.pattern = arg.slice('--pattern='.length);
      continue;
    }
    if (arg === '--kind' || arg === '-k') {
      opts.kind = argv[++i];
      continue;
    }
    if (arg.startsWith('--kind=')) {
      opts.kind = arg.slice('--kind='.length);
      continue;
    }
    if (arg === '--preset') {
      opts.preset = argv[++i];
      continue;
    }
    if (arg.startsWith('--preset=')) {
      opts.preset = arg.slice('--preset='.length);
      continue;
    }
    if (arg === '--rule') {
      const raw = argv[++i];
      try {
        opts.rule = JSON.parse(raw) as NapiConfig;
      } catch {
        throw new Error(
          `Invalid --rule JSON: ${raw?.slice(0, 100) ?? '(empty)'}`
        );
      }
      continue;
    }
    if (arg === '--root') {
      opts.root = path.resolve(argv[++i]);
      continue;
    }
    if (arg.startsWith('--root=')) {
      opts.root = path.resolve(arg.slice('--root='.length));
      continue;
    }
    if (arg === '--json') {
      opts.json = true;
      continue;
    }
    if (arg === '--limit') {
      opts.limit = parseInt(argv[++i], 10);
      continue;
    }
    if (arg === '--include-tests') {
      opts.includeTests = true;
      continue;
    }
    if (arg === '--context' || arg === '-C') {
      opts.context = parseInt(argv[++i], 10);
      continue;
    }
    if (arg === '--list-presets') {
      listPresets = true;
      continue;
    }
    if (arg === '--help' || arg === '-h') {
      printSearchHelp();
      process.exit(0);
    }
  }

  if (Number.isNaN(opts.limit)) opts.limit = 500;
  if (Number.isNaN(opts.context)) opts.context = 0;

  return { opts, listPresets };
}

function printSearchHelp(): void {
  console.log(`
ast-search — Structural code search powered by ast-grep

Usage:
  node scripts/ast/search.js [options]

Search modes (pick one):
  --pattern, -p <code>     Match code structurally (e.g. 'console.log($$$ARGS)')
  --kind, -k <kind>        Match AST node kind (e.g. 'function_declaration')
  --preset <name>          Use a built-in search preset (e.g. 'empty-catch')
  --rule <json>            Raw ast-grep rule object as JSON

Options:
  --root <path>            Search root directory (default: cwd)
  --json                   Output as JSON
  --limit N                Max matches (default: 500)
  --include-tests          Include test files
  --context, -C N          Lines of context around matches (text output only)
  --list-presets           Show available presets and exit
  --help, -h               Show this message

Pattern wildcards:
  $NAME                    Match any single AST node
  $$$NAME                  Match zero or more nodes (variadic)

Examples:
  node scripts/ast/search.js -p 'console.log($$$ARGS)' --root ./src
  node scripts/ast/search.js --preset empty-catch --root ./packages
  node scripts/ast/search.js -k function_declaration --json --limit 20
  node scripts/ast/search.js --preset todo-fixme --include-tests
  node scripts/ast/search.js -p 'if ($COND) { return $VAL }' --root ./src
  node scripts/ast/search.js --rule '{"rule":{"kind":"catch_clause"}}' --root ./src

Presets:
${Object.entries(PRESETS)
  .map(([name, p]) => `  ${name.padEnd(22)} ${p.description}`)
  .join('\n')}
`);
}

export function formatTextOutput(
  result: AstSearchResult,
  opts: AstSearchOptions,
  _root: string
): string {
  const lines: string[] = [];
  lines.push(`\n🔍 ${result.query}`);
  lines.push(
    `   ${result.totalMatches} matches across ${result.totalFiles} files\n`
  );

  const ctx = opts.context;
  const sourceMap = result._sourceByFile;

  let currentFile = '';
  for (const m of result.matches) {
    if (m.file !== currentFile) {
      currentFile = m.file;
      lines.push(`\n── ${currentFile} ──`);
    }

    if (ctx > 0 && sourceMap) {
      const srcLines = sourceMap.get(m.file);
      if (srcLines) {
        const start = Math.max(0, m.lineStart - 1 - ctx);
        const end = Math.min(srcLines.length, m.lineEnd + ctx);
        for (let i = start; i < end; i++) {
          const lineNum = i + 1;
          const marker =
            lineNum >= m.lineStart && lineNum <= m.lineEnd ? '>' : ' ';
          lines.push(
            `  ${marker} ${String(lineNum).padStart(4)} | ${srcLines[i]}`
          );
        }
        lines.push('');
        continue;
      }
    }

    const truncatedText =
      m.text.length > 200 ? m.text.slice(0, 200) + '…' : m.text;
    const singleLine = truncatedText.replace(/\n/g, '↵').replace(/\s+/g, ' ');
    lines.push(
      `  L${m.lineStart}:${m.columnStart}  [${m.kind}]  ${singleLine}`
    );

    if (m.metaVariables && Object.keys(m.metaVariables).length > 0) {
      for (const [k, v] of Object.entries(m.metaVariables)) {
        const truncV = v.length > 80 ? v.slice(0, 80) + '…' : v;
        lines.push(`    ${k} = ${truncV}`);
      }
    }
  }

  lines.push('');
  return lines.join('\n');
}

export async function main(): Promise<void> {
  const { opts, listPresets } = parseSearchArgs(process.argv.slice(2));

  if (listPresets) {
    if (opts.json) {
      console.log(JSON.stringify(PRESETS));
    } else {
      console.log('\nAvailable presets:\n');
      for (const [name, preset] of Object.entries(PRESETS)) {
        console.log(`  ${name.padEnd(26)} ${preset.description}`);
      }
      console.log('');
    }
    return;
  }

  if (!opts.pattern && !opts.kind && !opts.preset && !opts.rule) {
    console.error('Error: Must provide --pattern, --kind, --preset, or --rule');
    console.error('Run with --help for usage information.');
    process.exit(1);
  }

  const files = collectSearchFiles(opts.root, opts);

  if (files.length === 0) {
    console.error(`No files found in ${opts.root}`);
    process.exit(1);
  }

  const hasPython = files.some(f => isPythonFile(path.extname(f)));
  if (hasPython) {
    await ensurePythonRegistered();
  }

  const result = runSearch(files, opts, opts.root);

  if (opts.json) {
    console.log(JSON.stringify(result));
  } else {
    console.log(formatTextOutput(result, opts, opts.root));
  }
}

export { ensurePythonRegistered };
