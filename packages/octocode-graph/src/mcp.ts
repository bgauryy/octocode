import { runGraphCli } from './cli.ts';

/** Serve graph tools on the invoking process's stdin and stdout. */
export function startMcp(): Promise<number> {
  return runGraphCli(['mcp']);
}
