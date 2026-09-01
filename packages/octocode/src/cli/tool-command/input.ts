// Parses/validates raw CLI args into a tool's JSON input text and flags
// known input footguns before the tool actually runs.
import type { ParsedArgs } from '../types.js';
import { DirectToolInputError } from '@octocodeai/octocode-tools-core/schema';
import { formatToolExampleCommand } from './formatting.js';

const TOOL_RUNTIME_OPTION_KEYS = new Set([
  'queries',
  'json',
  'help',
  'version',
  'scheme',
  'compact',
  'pretty',
  'format',
  'full',
  'no-color',
]);

function getUnexpectedToolOptionKeys(args: ParsedArgs): string[] {
  return Object.keys(args.options).filter(
    key => !TOOL_RUNTIME_OPTION_KEYS.has(key)
  );
}

export function getInputText(
  toolName: string,
  args: ParsedArgs
): string | undefined {
  const unexpectedOptionKeys = getUnexpectedToolOptionKeys(args);
  if (unexpectedOptionKeys.length > 0) {
    const formattedKeys = unexpectedOptionKeys
      .map(key => `--${key}`)
      .join(', ');

    throw new DirectToolInputError(
      `Unsupported tool flags: ${formattedKeys}. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  if (args.args.length > 1) {
    throw new DirectToolInputError(
      `Pass tool input with --queries. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  if (typeof args.options.queries === 'string') return args.options.queries;
  return undefined;
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
  if (toolName !== 'localSearch') return;

  let rawPayload: unknown;
  try {
    rawPayload = JSON.parse(inputText) as unknown;
  } catch {
    return;
  }

  // Catch the common array/string mismatch before schema parsing so the error
  // names the canonical localSearch field directly.
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
    'localSearch does not accept keywords; set searchText to one string.',
    [
      'Use {"operation":"text","path":"/ABS/repo","searchText":"runCLI"} for localSearch.',
      `Run tools ${toolName} --scheme --brief before raw calls.`,
    ]
  );
}
