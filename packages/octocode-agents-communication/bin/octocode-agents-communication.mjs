#!/usr/bin/env node
const args = process.argv.slice(2);
try {
  const cli = args[0] === '/cli';
  const entry = await import(cli ? '../dist/cli.js' : '../dist/mcp.js');
  process.exitCode = await (cli ? entry.runCommunicationCli(args.slice(1)) : entry.runCommunicationMcp(args));
} catch (error) {
  console.error(error.message);
  process.exitCode = error.exitCode ?? 1;
}
