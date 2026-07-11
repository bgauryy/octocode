// Parses/validates raw CLI args into a tool's JSON input text and flags
// known input footguns before the tool actually runs.
import type { ParsedArgs } from '../types.js';
import { DirectToolInputError } from '@octocodeai/octocode-tools-core/schema';
import { formatToolExampleCommand } from './formatting.js';

const TOOL_RUNTIME_OPTION_KEYS = new Set([
  'queries',
  'query', // alias for --queries (the OQL `search --query` flag agents reach for)
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

  // Accept `--query` as an alias for `--queries`: `--query` is the OQL flag on
  // `search`, so agents routinely reach for it on raw tools too. Don't make them
  // pay for the easy-to-conflate name — treat both as the queries payload.
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

  const badIndex = getPayloadQueries(rawPayload).findIndex(
    query =>
      query &&
      typeof query === 'object' &&
      Array.isArray((query as { readonly keywords?: unknown }).keywords)
  );
  if (badIndex === -1) return;

  throw new DirectToolInputError(
    'localSearchCode.keywords must be a string, not an array.',
    [
      'Use {"path":".","keywords":"runCLI"} for localSearchCode.',
      'GitHub ghSearchCode uses keywords as an array; localSearchCode does not.',
      `Run tools ${toolName} --scheme before raw calls.`,
    ]
  );
}
