import { delegateToNative, resolveNativeBin } from 'octocode/native';

const usage = {
  html: 'octocode-graph html <saved-run> <new.html>',
  enrich: 'octocode-graph enrich <data.json> <manifest.json> <new-data.json>',
} as const;

export async function runGraphCli(args: readonly string[]): Promise<number> {
  const topic = args[0] === 'help' ? args[1] : args[0];
  if (
    (topic === 'html' || topic === 'enrich') &&
    args.length === 2 &&
    (args[0] === 'help' || args[1] === '--help' || args[1] === '-h')
  ) {
    process.stdout.write(`Usage: ${usage[topic]}\n`);
    return 0;
  }
  if (args[0] === 'html') {
    if (args.length !== 3) throw new Error(`Usage: ${usage.html}`);
    const { exportHtml } = await import('./html.js');
    await exportHtml(args[1], args[2]);
    return 0;
  }
  if (args[0] === 'enrich') {
    if (args.length !== 4) throw new Error(`Usage: ${usage.enrich}`);
    const { enrichExport } = await import('./enrich.js');
    await enrichExport(args[1], args[2], args[3]);
    return 0;
  }
  const engine = await resolveNativeBin();
  if ('error' in engine) throw new Error(engine.error);
  // Installing the graph server opts into the native graph tools. Workspace restrictions
  // and the host's enabled/disabled tool lists still apply to every MCP request.
  const env =
    args.includes('mcp') && process.env.OCTOCODE_BETA === undefined
      ? { ...process.env, OCTOCODE_BETA: 'true' }
      : process.env;
  const code = await delegateToNative(engine.bin, ['__graph', ...args], env);
  if (
    code === 0 &&
    args.length === 1 &&
    ['--help', '-h', 'help'].includes(args[0])
  ) {
    process.stdout.write(
      `\nSaved graph commands\n  ${usage.html}\n  ${usage.enrich}\n`
    );
  }
  return code;
}
