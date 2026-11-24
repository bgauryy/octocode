#!/usr/bin/env node
// `octocode-graph check`: dependency rules over a saved graph (check.ts), for CI: exit 1 on a new error violation.
import { parseArgs } from 'node:util';
import { CheckError, FORMATS, runCheck, type CheckFormat } from '../check.ts';
import { graphCommand } from '../invocation.ts';

const usage = (
  cli = graphCommand()
) => `usage: ${cli} check [<result-dir|project-dir>] [--rules <file>] [--format text|json|sarif]
                [--baseline <file>] [--write-baseline] [--since <git-rev>]
  Evaluates a rules file over the newest saved graph of a project directory (default .) or a result directory:
  forbidden, allowed and required rules in dependency-cruiser's vocabulary (paths as regular expressions over file
  ids and pkg:<name>; reachable, circular with via/viaOnly, dependencyTypes, $1 captures, a load per rule). Rules:
  .octocode/graph-rules.json at the graph root unless --rules. A baseline (graph-rules.baseline.json beside the rules,
  or --baseline) marks known violations, which never fail; --write-baseline records every current one. --since
  <rev> refreshes the graph's changed chunks and reports only violations the change introduced (git is only read).
  --format sarif writes SARIF 2.1.0 with each violation's chain as codeFlows. Exit 1 on an error violation the
  baseline does not list, 2 when the rules, baseline or arguments cannot be used, else 0.`;

async function main(argv: string[]) {
  let values: {
    rules?: string;
    baseline?: string;
    format?: string;
    since?: string;
    'write-baseline'?: boolean;
    help?: boolean;
  };
  let positionals: string[];
  try {
    ({ values, positionals } = parseArgs({
      args: argv,
      options: {
        rules: { type: 'string' },
        baseline: { type: 'string' },
        format: { type: 'string' },
        since: { type: 'string' },
        'write-baseline': { type: 'boolean' },
        help: { type: 'boolean', short: 'h' },
      },
      allowPositionals: true,
    }));
    if (values.help) {
      process.stdout.write(`${usage()}\n`);
      return 0;
    }
    if (positionals.length > 1)
      throw new Error('Expected one result or project directory');
    if (values.format && !FORMATS.includes(values.format as CheckFormat))
      throw new Error(
        `--format takes ${FORMATS.join(', ')}, not ${values.format}`
      );
  } catch (error) {
    process.stderr.write(
      `octocode-graph check: ${error instanceof Error ? error.message : String(error)}\n${usage()}\n`
    );
    return 2;
  }
  try {
    const { output, exitCode } = await runCheck({
      graph: positionals[0] ?? '.',
      ...(values.rules !== undefined && { rules: values.rules }),
      ...(values.baseline !== undefined && { baseline: values.baseline }),
      ...(values.since !== undefined && { since: values.since }),
      writeBaseline: values['write-baseline'] ?? false,
      format: (values.format ?? 'text') as CheckFormat,
      onNote: text => process.stderr.write(`octocode-graph check: ${text}\n`),
    });
    process.stdout.write(`${output}\n`);
    return exitCode;
  } catch (error) {
    if (!(error instanceof CheckError)) throw error;
    process.stderr.write(`octocode-graph check: ${error.message}\n`);
    return 2;
  }
}

process.exitCode = await main(process.argv.slice(2));
