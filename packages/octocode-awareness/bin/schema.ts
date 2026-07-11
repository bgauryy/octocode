import { runSchemaCli } from '../src/schema/cli.js';

try {
  process.exitCode = await runSchemaCli(process.argv.slice(2));
} catch (error: unknown) {
  console.log(JSON.stringify({
    ok: false,
    error_code: 'SCHEMA_RUNTIME_ERROR',
    error: error instanceof Error ? error.message : String(error),
  }, null, 2));
  process.exitCode = 1;
}
