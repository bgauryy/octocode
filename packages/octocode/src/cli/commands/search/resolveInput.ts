import { readFileSync } from 'node:fs';
import type { ParsedArgs } from '../../types.js';
import { getString, getBool } from '../../options.js';
import { parseOqlQueryJson, looksLikeJsonText } from './inputParsing.js';
import {
  isPullRequestTextQuery,
  isPullRequestPatchPath,
} from './prShorthand.js';
import { buildSugar } from './buildSugar.js';
import type { Resolved } from './types.js';

/** Resolve the OQL input from full JSON sources or CLI shorthand sugar. */
export function resolveInput(args: ParsedArgs): Resolved {
  // 1. Explicit JSON sources.
  const jsonText = readJsonText(args);
  if (jsonText && 'text' in jsonText) {
    try {
      return { input: parseOqlQueryJson(jsonText.text) };
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
  if (query && !isPullRequestTextQuery(args, query)) return { text: query };
  const file = getString(options, 'file');
  if (file && !isPullRequestPatchPath(args, file)) {
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
  if (first && looksLikeJsonText(first)) return { text: first };
  return undefined;
}
