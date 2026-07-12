import type { CLICommand } from '../types.js';
import { getBool } from '../options.js';
import { EXIT } from '../exit-codes.js';
import { printCliError } from '../cli-error.js';
import { render, renderRawContent, renderRows } from './search-render.js';
import {
  runOqlSearch,
  oqlSchemaText,
  oqlCompactSchemeText,
  oqlCompactSchemeJson,
  sanitizeStructuredContent,
  type OqlRunResult,
} from '@octocodeai/octocode-tools-core/oql';
import { normalizeTargetAlias } from './search/inputParsing.js';
import { tryHandleSymbolOutline } from './search/symbolOutline.js';
import { resolveInput } from './search/resolveInput.js';
import { exitCodeFor } from './search/exitCode.js';

/**
 * `octocode search` — the universal OQL runner. Thin: read the query (full OQL
 * JSON or CLI shorthand), delegate to runOqlSearch in tools-core, render the
 * typed envelope. No OQL logic lives here (the brain owns it); shorthand only
 * builds the sugar object the core normalizer already accepts.
 *
 * The argv/shorthand-resolution machinery that leads up to the OQL input lives
 * under ./search/ (target inference, corpus resolution, PR-ref shorthand,
 * input parsing, and the buildSugar orchestrator) — this file wires argv
 * options, delegates to it, and renders the resulting envelope.
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
    { name: 'raw' },
    { name: 'quiet' },
    { name: 'concise' },
    // shorthand sugar (normalized by core, shown canonical via --explain)
    { name: 'target', hasValue: true },
    { name: 'op', hasValue: true },
    { name: 'view', hasValue: true },
    { name: 'content-view', hasValue: true },
    { name: 'search', hasValue: true },
    { name: 'limit', hasValue: true },
    { name: 'page', hasValue: true },
    { name: 'items-per-page', hasValue: true },
    { name: 'pattern', hasValue: true },
    { name: 'rule', hasValue: true },
    { name: 'regex', hasValue: true },
    { name: 'pcre2' },
    { name: 'lang', hasValue: true },
    { name: 'repo', hasValue: true },
    { name: 'source', hasValue: true },
    { name: 'path', hasValue: true },
    { name: 'materialize', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'force-refresh' },
    { name: 'include', hasValue: true },
    { name: 'exclude', hasValue: true },
    { name: 'ext', hasValue: true },
    { name: 'exclude-dir', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'filename', hasValue: true },
    { name: 'path-pattern', hasValue: true },
    { name: 'entry', hasValue: true },
    { name: 'min-depth', hasValue: true },
    { name: 'max-depth', hasValue: true },
    { name: 'empty' },
    { name: 'modified-within', hasValue: true },
    { name: 'modified-before', hasValue: true },
    { name: 'accessed-within', hasValue: true },
    { name: 'size-greater', hasValue: true },
    { name: 'size-less', hasValue: true },
    { name: 'permissions', hasValue: true },
    { name: 'executable' },
    { name: 'readable' },
    { name: 'writable' },
    { name: 'details' },
    { name: 'show-modified' },
    { name: 'hidden' },
    { name: 'no-ignore' },
    { name: 'case-insensitive' },
    { name: 'case-sensitive' },
    { name: 'whole-word' },
    { name: 'fixed' },
    { name: 'multiline' },
    { name: 'multiline-dotall' },
    { name: 'files-only' },
    { name: 'files-without-match' },
    { name: 'count-lines' },
    { name: 'count-matches' },
    { name: 'only-matching' },
    { name: 'unique' },
    { name: 'count' },
    { name: 'context-lines', hasValue: true },
    { name: 'context', hasValue: true },
    { name: 'invert-match' },
    { name: 'match-window', hasValue: true },
    { name: 'match-length', hasValue: true },
    { name: 'max-matches', hasValue: true },
    { name: 'match-page', hasValue: true },
    { name: 'max-files', hasValue: true },
    { name: 'sort', hasValue: true },
    { name: 'sort-reverse' },
    { name: 'ranking-profile', hasValue: true },
    { name: 'debug-ranking' },
    { name: 'match-string', hasValue: true },
    { name: 'match-regex' },
    { name: 'match-case-sensitive' },
    { name: 'start-line', hasValue: true },
    { name: 'end-line', hasValue: true },
    { name: 'char-offset', hasValue: true },
    { name: 'char-length', hasValue: true },
    { name: 'full-content' },
    { name: 'tree' },
    { name: 'include-sizes' },
    { name: 'symbols' },
    { name: 'kind', hasValue: true },
    { name: 'symbol', hasValue: true },
    { name: 'uri', hasValue: true },
    { name: 'line', hasValue: true },
    { name: 'order', hasValue: true },
    { name: 'depth', hasValue: true },
    { name: 'workspace-root', hasValue: true },
    { name: 'format', hasValue: true },
    { name: 'owner', hasValue: true },
    { name: 'topic', hasValue: true },
    { name: 'stars', hasValue: true },
    { name: 'forks', hasValue: true },
    { name: 'good-first-issues', hasValue: true },
    { name: 'license', hasValue: true },
    { name: 'created', hasValue: true },
    { name: 'updated', hasValue: true },
    { name: 'closed', hasValue: true },
    { name: 'merged-at', hasValue: true },
    { name: 'size', hasValue: true },
    { name: 'match', hasValue: true },
    { name: 'archived', hasValue: true },
    { name: 'visibility', hasValue: true },
    { name: 'state', hasValue: true },
    { name: 'author', hasValue: true },
    { name: 'label', hasValue: true },
    { name: 'pr', hasValue: true },
    { name: 'base', hasValue: true },
    { name: 'head', hasValue: true },
    { name: 'draft' },
    { name: 'comments' },
    { name: 'commits' },
    { name: 'deep' },
    { name: 'review-mode', hasValue: true },
    { name: 'file-page', hasValue: true },
    { name: 'comment-page', hasValue: true },
    { name: 'commit-page', hasValue: true },
    { name: 'since', hasValue: true },
    { name: 'until', hasValue: true },
    { name: 'patches' },
    { name: 'base-ref', hasValue: true },
    { name: 'head-ref', hasValue: true },
    { name: 'detailed' },
    { name: 'offsets' },
    { name: 'verbose' },
    { name: 'intent', hasValue: true },
    { name: 'facets', hasValue: true },
    { name: 'proof', hasValue: true },
    { name: 'proof-limit', hasValue: true },
    { name: 'include-packets' },
    { name: 'include-facts' },
    { name: 'include-edges' },
  ],
  handler: async (args): Promise<void> => {
    const { options } = args;

    // Accept friendly `--target` abbreviations (the ones the help/agent prompt
    // use, e.g. `repos`, `PRs`) and fold them to the canonical OQL enum so an
    // agent copying the help string doesn't hit "must be one of …".
    normalizeTargetAlias(options);

    // --scheme: print the OQL schema and exit. --scheme --compact prints the
    // lean agent guide (TEXT); --scheme --json --compact prints the same guide
    // as small machine-readable JSON. Plain --scheme --json remains full.
    if (getBool(options, 'scheme')) {
      const schemeText =
        getBool(options, 'json') && getBool(options, 'compact')
          ? oqlCompactSchemeJson()
          : getBool(options, 'compact')
            ? oqlCompactSchemeText()
            : oqlSchemaText();
      process.stdout.write(`${schemeText}\n`);
      return;
    }
    if (getBool(options, 'raw') && getBool(options, 'json')) {
      printCliError('Use either --raw or --json, not both.');
      process.exitCode = EXIT.USAGE;
      return;
    }
    if (
      getBool(options, 'quiet') &&
      (getBool(options, 'json') || getBool(options, 'raw'))
    ) {
      printCliError('Use --quiet alone (not with --json or --raw).');
      process.exitCode = EXIT.USAGE;
      return;
    }
    if (await tryHandleSymbolOutline(args)) {
      return;
    }

    const resolved = resolveInput(args);
    if (resolved === undefined) {
      printCliError(
        'search needs a query. Shorthand: `search "<text>" [path|owner/repo]` (text/regex/AST), `search <dir|owner/repo> --tree`, `search <file|owner/repo/path>` (or --pattern/--rule/--regex with --lang). Full: --query <json> | --file <path> | --stdin. See `octocode search --scheme`.'
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

    // Redact secrets before ANY output path (json / raw / rendered). OQL
    // adapters return raw rows and rely on the interface layer to sanitize; the
    // MCP path does this via sanitizeCallToolResult, so the CLI must too — else
    // code snippets can leak tokens/keys to stdout.
    result = sanitizeStructuredContent(result) as OqlRunResult;

    if (getBool(options, 'json')) {
      // --compact emits single-line minified JSON (stream/parse friendly);
      // plain --json stays pretty-printed for human reading. Both are valid
      // JSON with control characters escaped.
      const json = getBool(options, 'compact')
        ? JSON.stringify(result)
        : JSON.stringify(result, null, 2);
      process.stdout.write(`${json}\n`);
    } else if (getBool(options, 'raw')) {
      const raw = renderRawContent(result);
      if (raw === undefined) {
        printCliError('--raw is only available for content result rows.');
        process.exitCode = EXIT.USAGE;
        return;
      }
      process.stdout.write(raw.endsWith('\n') ? raw : `${raw}\n`);
    } else if (getBool(options, 'quiet')) {
      // Rows only — no plan/diagnostics/evidence/continuations. Token-frugal
      // mode for agent loops that only need path:line anchors.
      const rows = renderRows(result);
      process.stdout.write(rows.length > 0 ? `${rows}\n` : '');
    } else {
      process.stdout.write(`${render(result, getBool(options, 'compact'))}\n`);
    }

    process.exitCode = exitCodeFor(result);
  },
};
