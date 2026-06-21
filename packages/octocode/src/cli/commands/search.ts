import { readFileSync } from 'node:fs';
import type { CLICommand, ParsedArgs } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { printCliError } from '../cli-error.js';
import { resolveRef, isGithubRef } from '../routing.js';
import {
  runOqlSearch,
  oqlSchemaText,
  buildShorthandInput,
  type ShorthandCorpus,
  type OqlResultEnvelope,
  type OqlRunResult,
  isBatchEnvelope,
} from '@octocodeai/octocode-tools-core/oql';

/**
 * `octocode search` — the universal OQL runner. Thin: read the query (full OQL
 * JSON or CLI shorthand), delegate to runOqlSearch in tools-core, render the
 * typed envelope. No OQL logic lives here (the brain owns it); shorthand only
 * builds the sugar object the core normalizer already accepts.
 */
export const searchCommand: CLICommand = {
  name: 'search',
  options: [
    { name: 'query', hasValue: true },
    { name: 'file', hasValue: true },
    { name: 'stdin' },
    { name: 'scheme' },
    { name: 'explain' },
    { name: 'dry-run' },
    { name: 'json' },
    { name: 'compact' },
    // shorthand sugar (normalized by core, shown canonical via --explain)
    { name: 'pattern', hasValue: true },
    { name: 'rule', hasValue: true },
    { name: 'regex', hasValue: true },
    { name: 'pcre2' },
    { name: 'lang', hasValue: true },
    { name: 'type', hasValue: true },
    { name: 'repo', hasValue: true },
    { name: 'materialize', hasValue: true },
  ],
  handler: async (args): Promise<void> => {
    const { options } = args;

    // --scheme: print the OQL schema and exit.
    if (getBool(options, 'scheme')) {
      process.stdout.write(`${oqlSchemaText()}\n`);
      return;
    }

    const resolved = resolveInput(args);
    if (resolved === undefined) {
      printCliError(
        'search needs a query. Shorthand: `search "<text>" [path|owner/repo]` (or --pattern/--rule/--regex with --lang/--type). Full: --query <json> | --file <path> | --stdin. See `octocode search --scheme`.'
      );
      process.exitCode = EXIT.USAGE;
      return;
    }
    if ('error' in resolved) {
      printCliError(resolved.error);
      process.exitCode = EXIT.USAGE;
      return;
    }
    const input = resolved.input;

    const explain = getBool(options, 'explain');
    const dryRun = getBool(options, 'dry-run');

    // --explain sets explain on the query so the plan is included in the
    // envelope; --dry-run plans without executing.
    const withExplain =
      explain && input && typeof input === 'object'
        ? { ...(input as Record<string, unknown>), explain: true }
        : input;

    let result: OqlRunResult;
    try {
      result = await runOqlSearch(withExplain as never, { dryRun });
    } catch (err) {
      printCliError(`OQL execution failed: ${(err as Error).message}`);
      process.exitCode = EXIT.TOOL;
      return;
    }

    if (getBool(options, 'json')) {
      process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    } else {
      process.stdout.write(`${render(result, getBool(options, 'compact'))}\n`);
    }

    process.exitCode = exitCodeFor(result);
  },
};

type Resolved = { input: unknown } | { error: string } | undefined;

/** Resolve the OQL input from full JSON sources or CLI shorthand sugar. */
function resolveInput(args: ParsedArgs): Resolved {
  // 1. Explicit JSON sources.
  const jsonText = readJsonText(args);
  if (jsonText && 'text' in jsonText) {
    try {
      return { input: JSON.parse(jsonText.text) };
    } catch (err) {
      return {
        error: `Could not parse OQL query JSON: ${(err as Error).message}`,
      };
    }
  }
  if (jsonText && 'error' in jsonText) return { error: jsonText.error };

  // 2. Shorthand sugar -> the sugar object the core normalizer accepts.
  return buildSugar(args);
}

function readJsonText(
  args: ParsedArgs
): { text: string } | { error: string } | undefined {
  const { options } = args;
  const query = getString(options, 'query');
  if (query) return { text: query };
  const file = getString(options, 'file');
  if (file) {
    try {
      return { text: readFileSync(file, 'utf8') };
    } catch (err) {
      return {
        error: `Could not read --file ${file}: ${(err as Error).message}`,
      };
    }
  }
  if (getBool(options, 'stdin')) {
    try {
      return { text: readFileSync(0, 'utf8') };
    } catch {
      return { error: 'Could not read OQL query from stdin.' };
    }
  }
  // bare positional JSON, e.g. `search '{...}'`
  const first = args.args[0];
  if (first && first.trim().startsWith('{')) return { text: first };
  return undefined;
}

/**
 * Read shorthand from argv and delegate the lowering to tools-core's
 * `buildShorthandInput`. The CLI owns ONLY argv parsing and resolving the
 * target string to a corpus (the filesystem-dependent step); predicate
 * selection / dialect / assembly live in the brain.
 *   search "<term>" [path|owner/repo]            -> text
 *   search --regex '<re>' [target] [--pcre2]     -> regex
 *   search --pattern '<shape>' [target] --lang t -> structural pattern
 *   search --rule '<json>' [target] --lang t     -> structural rule
 */
function buildSugar(args: ParsedArgs): Resolved {
  const { options } = args;
  const positionals = args.args.filter(a => !a.startsWith('-'));
  const pattern = getString(options, 'pattern') || undefined;
  const ruleText = getString(options, 'rule') || undefined;
  const regex = getString(options, 'regex') || undefined;

  // When the predicate comes from a flag the positional is the TARGET; for a
  // bare text term positional[0] is the term and [1] the target.
  const fromFlag = Boolean(pattern || ruleText || regex);
  const text = fromFlag ? undefined : positionals[0];
  const targetArg = getString(options, 'repo') || positionals[fromFlag ? 0 : 1];

  if (!fromFlag && !text) return undefined; // nothing to search for

  let rule: unknown;
  if (ruleText !== undefined) {
    try {
      rule = JSON.parse(ruleText);
    } catch (err) {
      return { error: `--rule must be JSON: ${(err as Error).message}` };
    }
  }

  const corpus = resolveCorpus(targetArg);
  const result = buildShorthandInput({
    ...(text !== undefined ? { text } : {}),
    ...(regex !== undefined ? { regex } : {}),
    ...(getBool(options, 'pcre2') ? { pcre2: true } : {}),
    ...(pattern !== undefined ? { pattern } : {}),
    ...(rule !== undefined ? { rule } : {}),
    ...(getString(options, 'lang') ? { lang: getString(options, 'lang') } : {}),
    ...(getString(options, 'type') ? { type: getString(options, 'type') } : {}),
    corpus,
    ...(getString(options, 'materialize')
      ? { materialize: getString(options, 'materialize') as never }
      : {}),
  });
  return 'error' in result ? { error: result.error } : { input: result.input };
}

/** Resolve a target string to a corpus (local path vs GitHub ref). FS-aware. */
function resolveCorpus(target: string | undefined): ShorthandCorpus {
  if (!target) return { kind: 'local', path: '.' };
  const ref = resolveRef(target);
  if (isGithubRef(ref)) {
    return {
      kind: 'github',
      repo: `${ref.owner}/${ref.repo}`,
      ...(ref.subpath ? { path: ref.subpath } : {}),
      ...(ref.branch ? { ref: ref.branch } : {}),
    };
  }
  return { kind: 'local', path: ref.path };
}

function exitCodeFor(result: OqlRunResult): number {
  const envelopes = isBatchEnvelope(result)
    ? result.children.map(c => c.envelope)
    : [result];
  for (const env of envelopes) {
    if (env.evidence.kind === 'unsupported') return EXIT.TOOL;
    if (env.diagnostics.some(d => d.code === 'rateLimited'))
      return EXIT.RATE_LIMIT;
    if (env.diagnostics.some(d => d.code === 'invalidQuery')) return EXIT.USAGE;
  }
  return EXIT.OK;
}

/* ------------------------------ rendering ------------------------------- */

function render(result: OqlRunResult, compact: boolean): string {
  if (isBatchEnvelope(result)) {
    const parts = result.children.map(
      child =>
        `${bold(c('cyan', `# query ${child.queryIndex} (${child.queryId})`))}\n` +
        renderEnvelope(child.envelope, compact)
    );
    if (result.merged) {
      parts.push(
        `${bold(c('cyan', '# merged'))}\n` +
          renderEnvelope(result.merged, compact)
      );
    }
    for (const d of result.diagnostics) {
      parts.push(dim(`! ${d.code}: ${d.message}`));
    }
    return parts.join('\n\n');
  }
  return renderEnvelope(result, compact);
}

function renderEnvelope(env: OqlResultEnvelope, compact: boolean): string {
  const lines: string[] = [];

  if (env.plan) {
    lines.push(bold(c('magenta', 'PLAN')));
    for (const node of env.plan.nodes) {
      lines.push(
        `  ${node.path}  ${routeColor(node.route)}${node.backend ? dim(` -> ${node.backend}`) : ''}`
      );
      if (!compact) lines.push(dim(`    ${node.reason}`));
    }
    if (env.plan.materialization) {
      lines.push(
        dim(
          `  materialize: ${env.plan.materialization.mode} (${env.plan.materialization.reason})`
        )
      );
    }
    lines.push('');
  }

  for (const row of env.results) {
    lines.push(renderRow(row));
  }

  if (env.results.length === 0 && !env.plan) {
    lines.push(dim('  (no results)'));
  }

  if (env.pagination?.hasMore) {
    lines.push(dim('  … more results available (follow next.page)'));
  }

  for (const d of env.diagnostics) {
    const sev =
      d.severity === 'error'
        ? c('red', '✗')
        : d.severity === 'warning'
          ? c('yellow', '!')
          : dim('·');
    lines.push(`  ${sev} ${dim(d.code)}: ${d.message}`);
  }

  const ev = env.evidence;
  lines.push(
    dim(
      `  evidence: ${ev.kind}  answerReady=${ev.answerReady}  complete=${ev.complete}`
    )
  );
  return lines.join('\n');
}

function renderRow(row: OqlResultEnvelope['results'][number]): string {
  switch (row.kind) {
    case 'code':
      return `  ${c('green', row.path)}${row.line !== undefined ? `:${row.line}` : ''}${row.snippet ? `  ${dim(row.snippet.trim().slice(0, 200))}` : ''}`;
    case 'file':
      return `  ${c('green', row.path)}${row.entryType === 'directory' ? '/' : ''}`;
    case 'tree':
      return `  ${row.entryType === 'directory' ? c('blue', row.path) + '/' : c('green', row.path)}`;
    case 'content':
      return `  ${c('green', row.path)} [${row.contentView}]\n${row.content}`;
    case 'record':
      return renderRecord(row);
  }
}

/** Render a V2 record row meaningfully per recordType (id + key fields). */
function renderRecord(row: {
  recordType: string;
  id?: string;
  data: Record<string, unknown>;
}): string {
  const d = row.data;
  const get = (k: string): string | undefined =>
    d[k] === undefined || d[k] === null ? undefined : String(d[k]);
  const head = `  ${c('cyan', row.recordType)} ${c('green', row.id ?? '(no id)')}`;
  let detail = '';
  switch (row.recordType) {
    case 'repository':
      detail = [
        get('stars') && `★${get('stars')}`,
        get('language'),
        get('description'),
      ]
        .filter(Boolean)
        .join('  ');
      break;
    case 'package':
      detail = [get('description'), get('repository')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'pullRequest':
      detail = [get('state'), get('title'), get('author')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'commit':
      detail = [get('title') ?? get('messageHeadline'), get('author')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'artifact':
      detail = [get('mode'), get('format'), get('arch')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'diff':
      detail = [
        get('path') ?? get('filename'),
        get('additions') && `+${get('additions')}`,
        get('deletions') && `-${get('deletions')}`,
      ]
        .filter(Boolean)
        .join('  ');
      break;
    case 'semantics':
      detail = [get('name') ?? get('symbolName'), get('kind')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'research':
      detail = renderResearchRecord(d);
      break;
  }
  return detail ? `${head}  ${dim(detail.slice(0, 200))}` : head;
}

function renderResearchRecord(d: Record<string, unknown>): string {
  const summary =
    d.summary && typeof d.summary === 'object' && !Array.isArray(d.summary)
      ? (d.summary as Record<string, unknown>)
      : {};
  const n = (key: string): string | undefined =>
    typeof summary[key] === 'number' ? String(summary[key]) : undefined;
  const parts = [
    typeof d.intent === 'string' ? `intent=${d.intent}` : undefined,
    n('sourceFiles') && `files=${n('sourceFiles')}`,
    n('unusedFiles') && `unusedFiles=${n('unusedFiles')}`,
    n('exportedSymbols') && `symbols=${n('exportedSymbols')}`,
    n('candidateUnusedExports') &&
      `candidateExports=${n('candidateUnusedExports')}`,
    n('transitiveDeadExports') &&
      `transitiveDead=${n('transitiveDeadExports')}`,
    n('unlistedDependencies') && `unlistedDeps=${n('unlistedDependencies')}`,
    n('unusedDependencies') && `unusedDeps=${n('unusedDependencies')}`,
    n('duplicateDependencies') && `duplicateDeps=${n('duplicateDependencies')}`,
  ].filter(Boolean);
  return parts.join('  ');
}

function routeColor(route: string): string {
  switch (route) {
    case 'PUSHDOWN':
      return c('green', route);
    case 'ROUTE':
      return c('cyan', route);
    case 'RESIDUAL':
      return c('yellow', route);
    default:
      return c('red', route);
  }
}
