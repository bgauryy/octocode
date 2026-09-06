import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import type {
  StructuralDiagnostic,
  StructuralSearchFilesResult,
} from '@octocodeai/octocode-engine';
import type { LocalSearchCodeData } from './scheme.js';

type StructuralToolResult = LocalSearchCodeToolResult &
  Pick<
    LocalSearchCodeData,
    'truncated' | 'terminalLimit' | 'partialReasons' | 'diagnostics'
  >;

export function isIncomplete(result: StructuralSearchFilesResult): boolean {
  return (
    result.scanTruncated ||
    result.status === 'truncated' ||
    result.status === 'partial'
  );
}

export function withCompleteness(
  result: LocalSearchCodeToolResult,
  status: string,
  diagnostics: StructuralDiagnostic[]
): StructuralToolResult {
  const output = result as StructuralToolResult;
  if (diagnostics.length) output.diagnostics = diagnostics;
  if (status === 'truncated' || status === 'partial') {
    output.truncated = true;
    // These native limits have no resumable cursor or user-adjustable budget.
    // File/match pagination still applies to any completed results.
    output.terminalLimit = true;
    output.partialReasons = [
      ...(output.partialReasons ?? []),
      status === 'truncated' ? 'structuralLimit' : 'skippedFiles',
    ];
  }
  return output;
}

export function executionLimitDiagnostic(
  message: string,
  path: string
): StructuralDiagnostic | undefined {
  const code =
    /^\[(structural\.(?:match\.(?:depthLimit|backtrackingLimit|deadline)|parse\.interrupted|content\.tooLarge))\]/.exec(
      message
    )?.[1];
  if (!code) return undefined;
  return {
    code,
    severity: 'warning',
    stage: code.startsWith('structural.match.') ? 'match' : 'parse',
    message,
    path,
    recovery:
      'Narrow the source or simplify the query. This execution was incomplete; zero matches does not prove absence.',
  };
}

export function rewrittenQueryDiagnostic(
  requestedPattern: string | undefined,
  executedPattern: string | undefined,
  path: string
): StructuralDiagnostic | undefined {
  if (!executedPattern || executedPattern === requestedPattern)
    return undefined;
  return {
    code: 'structural.query.rewritten',
    severity: 'info',
    stage: 'match',
    message: `Executed pattern ${JSON.stringify(executedPattern)} after the requested pattern ${JSON.stringify(requestedPattern)} returned no matches.`,
    path,
    recovery: `Use pattern: ${JSON.stringify(executedPattern)} to repeat this query explicitly.`,
  };
}
