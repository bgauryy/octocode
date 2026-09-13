import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';
import { buildArgs, decodeMatches, scanSucceeded } from './astGrep.js';
import { rewriteError } from './result.js';
import type {
  AstRewriteError,
  AstRewriteQuery,
  AstRewriteRuntimeDeps,
  PreparedFile,
} from './types.js';

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES = 10 * 1024 * 1024;

export async function evaluatePostconditions(
  query: AstRewriteQuery,
  executable: string,
  files: PreparedFile[],
  deps: AstRewriteRuntimeDeps
): Promise<
  | { ok: true; remainingMatches: number }
  | { ok: false; result: AstRewriteError }
> {
  if (!query.postconditions || query.postconditions.length === 0)
    return { ok: true, remainingMatches: 0 };

  const mirror = await mkdtemp(
    join(tmpdir(), 'octocode-ast-rewrite-postcondition-')
  );
  try {
    for (const file of files) {
      const target = join(mirror, file.path);
      await mkdir(dirname(target), { recursive: true });
      await writeFile(target, file.after, { mode: file.mode });
    }
    const execution = await spawnWithTimeout(
      executable,
      buildArgs(query, mirror),
      {
        cwd: mirror,
        timeout: deps.timeoutMs ?? DEFAULT_TIMEOUT_MS,
        maxOutputSize: deps.maxProcessOutputBytes ?? DEFAULT_MAX_OUTPUT_BYTES,
      }
    );
    if (!scanSucceeded(execution)) {
      return {
        ok: false,
        result: rewriteError(
          'ast.rewrite.postcondition_execution_failed',
          'The postcondition scan could not be completed; no files were changed.'
        ),
      };
    }
    const matches = decodeMatches(execution.stdout);
    if (!matches) {
      return {
        ok: false,
        result: rewriteError(
          'ast.rewrite.postcondition_output_invalid',
          'The postcondition scan returned invalid output; no files were changed.'
        ),
      };
    }
    for (const postcondition of query.postconditions) {
      if (matches.length !== postcondition.equals) {
        return {
          ok: false,
          result: rewriteError(
            'ast.rewrite.postcondition_failed',
            'A staged rewrite postcondition failed; no files were changed.',
            {
              details: {
                kind: postcondition.kind,
                expected: postcondition.equals,
                observed: matches.length,
              },
            }
          ),
        };
      }
    }
    return { ok: true, remainingMatches: matches.length };
  } finally {
    await rm(mirror, { recursive: true, force: true });
  }
}
