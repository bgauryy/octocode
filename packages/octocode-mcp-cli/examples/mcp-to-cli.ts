import { realpathSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import type { Client } from '@modelcontextprotocol/client';
import { z } from 'zod';
import {
  cliFromMcp,
  connectStdio,
  defineCli,
  defineCommand,
  mcpServerOptions,
  registerOn,
  runCli,
  type CliSpec,
} from 'octocode-mcp-cli';

export async function cliWithLocalCommands(client: Client): Promise<CliSpec> {
  const imported = await cliFromMcp(client);
  let spec = imported;
  const doctor = defineCommand({
    name: 'cli-doctor',
    title: 'CLI doctor',
    description: 'List the commands on this CLI.',
    annotations: { readOnlyHint: true },
    schema: z.object({}),
    run: () => spec.commands.map(command => command.name),
  });
  spec = defineCli({ ...imported, commands: [...imported.commands, doctor] });
  return spec;
}

async function main(): Promise<void> {
  const expose = process.argv[2] === '--expose';
  const client = await connectStdio({
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-everything'],
    // Exposed over stdio, the server's stderr must not interleave with this process's MCP stream.
    stderr: expose ? 'ignore' : 'inherit',
  });
  try {
    const spec = await cliWithLocalCommands(client);
    if (!expose) {
      process.exitCode = await runCli(spec, process.argv.slice(2));
      return;
    }
    const server = new McpServer({ name: spec.name, version: spec.version }, mcpServerOptions(spec));
    registerOn(server as unknown as Parameters<typeof registerOn>[0], spec);
    const inbound = new StdioServerTransport();
    await server.connect(inbound);
    await new Promise<void>(resolve => {
      process.stdin.once('end', () => resolve());
      process.stdin.once('close', () => resolve());
    });
    await server.close();
  } finally {
    await client.close();
  }
}

function ranAsScript(): boolean {
  const entry = process.argv[1];
  if (!entry) return false;
  try {
    return realpathSync(fileURLToPath(import.meta.url)) === realpathSync(entry);
  } catch {
    return import.meta.url === pathToFileURL(entry).href;
  }
}

if (ranAsScript()) {
  await main();
}
