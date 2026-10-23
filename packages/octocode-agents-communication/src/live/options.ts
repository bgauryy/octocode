// Live-harness command line: usage text, the options it accepts and their parser.

export const HELP = `Usage: node src/live.ts --live [--scenarios LIST] [options]

Scenarios (comma-separated --scenarios; default qa,lock-handoff,paged-read):
  qa            passive FYI, then an arithmetic request: exact reply, both acknowledgements, host submission
  lock-handoff  worker asks the lease owner, acquires src/api.ts only after release, answers exactly
  paged-read    worker follows every page of a shared document and answers with the final-page code
  soak          Pi, Claude and Codex together: concurrent request waves across idle intervals; runs alone

qa, lock-handoff and paged-read run --runs times each against ONE installed host (claude, codex or pi):
  --vendor NAME     host to run (default claude); codex falls back to its configured model
  --model ID        model ID available to that host (required except for codex)
  --runs N          trials per scenario (default 1)      --parallel N   concurrent trials (default 3)
  --timeout MS      per-trial deadline (default 240000)

soak needs all three hosts installed:
  --models JSON     explicit IDs, e.g. '{"pi":"P","claude":"C","codex":"X"}' (required)
  --waves N (2-10, default 3)  --idle-ms MS (60000-120000, default 65000)  --duration-ms MS (default 480000)

Shared: --live (required), --package DIR (package root holding dist/communication.js and OPERATING.md; default this package), --output DIR, --help
`;

export interface LiveOptions {
  live?: boolean; help?: boolean; flows?: string[]; models?: unknown;
  vendor?: string; model?: string; package?: string; output?: string; scenarios?: string;
  runs?: string | number; parallel?: string | number; timeout?: string | number;
  waves?: string | number; idleMs?: string | number; durationMs?: string | number;
}

type ValueField = 'vendor' | 'model' | 'package' | 'output' | 'scenarios' | 'runs' | 'parallel' | 'timeout' | 'waves' | 'idleMs' | 'durationMs';
const VALUE_FLAGS: Record<string, ValueField | 'models'> = { '--vendor': 'vendor', '--model': 'model', '--models': 'models', '--package': 'package', '--output': 'output', '--runs': 'runs', '--parallel': 'parallel', '--timeout': 'timeout', '--scenarios': 'scenarios', '--waves': 'waves', '--idle-ms': 'idleMs', '--duration-ms': 'durationMs' };

/** Parses argv into options; throws on an unknown flag, a missing value or unparsable --models. */
export function parseArgs(argv: string[]): LiveOptions {
  const options: LiveOptions = {};
  for (let index = 0; index < argv.length; index++) {
    const flag = argv[index];
    if (flag === '--live') { options.live = true; continue; }
    if (flag === '--help' || flag === '-h') { options.help = true; continue; }
    const field = VALUE_FLAGS[flag];
    if (!field || argv[index + 1] === undefined) throw new Error(`Unknown or incomplete option ${flag}`);
    const value = argv[++index] ?? '';
    if (field === 'models') { try { options.models = JSON.parse(value); } catch { throw new Error('--models must be a JSON object of pi, claude and codex model IDs'); } }
    else options[field] = value;
  }
  return options;
}
