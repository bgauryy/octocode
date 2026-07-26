// Parses/validates raw CLI args into a tool's JSON input text and flags
// known input footguns before the tool actually runs.
import type { ParsedArgs } from '../types.js';
import { DirectToolInputError } from '@octocodeai/octocode-tools-core/schema';
import { formatToolExampleCommand } from './formatting.js';

const TOOL_RUNTIME_OPTION_KEYS = new Set([
  'queries',
  'query', // alias for --queries (the singular name agents naturally reach for)
  'json',
  'help',
  'version',
  'list',
  'scheme',
  'compact',
  'format',
  'full',
  'no-color',
]);

function getUnexpectedToolOptionKeys(args: ParsedArgs): string[] {
  return Object.keys(args.options).filter(
    key => key !== 'input' && !TOOL_RUNTIME_OPTION_KEYS.has(key)
  );
}

export function getInputText(
  toolName: string,
  args: ParsedArgs
): string | undefined {
  if (args.options.input !== undefined) {
    throw new DirectToolInputError(
      `Legacy --input is not supported. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  const unexpectedOptionKeys = getUnexpectedToolOptionKeys(args);
  if (unexpectedOptionKeys.length > 0) {
    const formattedKeys = unexpectedOptionKeys
      .map(key => `--${key}`)
      .join(', ');

    throw new DirectToolInputError(
      `Unsupported tool flags: ${formattedKeys}. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  if (args.args.length > 2) {
    throw new DirectToolInputError(
      `Pass tool input as one quoted JSON string. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  // Accept `--query` as an alias for `--queries`: agents routinely reach for
  // the singular form. Don't make them pay for the easy-to-conflate name —
  // treat both as the queries payload.
  if (typeof args.options.queries === 'string') return args.options.queries;
  if (typeof args.options.query === 'string') return args.options.query;
  return args.args[1];
}

function getPayloadQueries(rawPayload: unknown): unknown[] {
  if (Array.isArray(rawPayload)) return rawPayload;
  if (rawPayload && typeof rawPayload === 'object') {
    const queries = (rawPayload as { readonly queries?: unknown }).queries;
    if (Array.isArray(queries)) return queries;
    return [rawPayload];
  }
  return [];
}

export function validateRawToolFootguns(
  toolName: string,
  inputText: string
): void {
  if (toolName !== 'localSearchCode') return;

  let rawPayload: unknown;
  try {
    rawPayload = JSON.parse(inputText) as unknown;
  } catch {
    return;
  }

  // A plain `keywords`/`keywordsToSearch` string is folded to `searchText` by
  // the alias layer; only an ARRAY can't (searchText is a single string), so
  // catch that here with a friendly redirect.
  const badIndex = getPayloadQueries(rawPayload).findIndex(query => {
    if (!query || typeof query !== 'object') return false;
    const q = query as {
      readonly searchText?: unknown;
      readonly keywords?: unknown;
    };
    return Array.isArray(q.searchText) || Array.isArray(q.keywords);
  });
  if (badIndex === -1) return;

  throw new DirectToolInputError(
    'localSearchCode.searchText must be a single string, not an array.',
    [
      'Use {"path":".","searchText":"runCLI"} for localSearchCode.',
      'ghSearchCode/ghSearchRepos use `keywords` (an array of ANDed terms); localSearchCode uses `searchText` (one text/regex string).',
      `Run tools ${toolName} --scheme before raw calls.`,
    ]
  );
}
