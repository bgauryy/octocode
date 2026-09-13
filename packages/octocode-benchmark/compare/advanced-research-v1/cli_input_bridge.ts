/**
 * One-purpose executable for the benchmark observer. It bundles the CLI's
 * canonical flag parser, then turns an argv tail into a single query without
 * registering or invoking any tool runtime.
 */
import { buildQueryFromFlags } from '../../../octocode/src/cli/tool-command/flags-to-query.ts';

const [toolName, ...tail] = process.argv.slice(2);

if (!toolName) {
  process.stdout.write(
    JSON.stringify({ ok: false, error: { message: 'A tool name is required.' } })
  );
  process.exitCode = 2;
} else {
  try {
    process.stdout.write(JSON.stringify({ ok: true, query: buildQueryFromFlags(toolName, tail) }));
  } catch (error) {
    const input = error as Error & { details?: unknown };
    process.stdout.write(
      JSON.stringify({
        ok: false,
        error: {
          name: input.name,
          message: input.message,
          ...(input.details === undefined ? {} : { details: input.details }),
        },
      })
    );
    process.exitCode = 2;
  }
}
