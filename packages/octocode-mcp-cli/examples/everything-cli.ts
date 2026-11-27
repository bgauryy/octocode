import { cliFromMcp, connectStdio, runCli } from 'octocode-mcp-cli';

// https://github.com/modelcontextprotocol/servers/tree/main/src/everything
const client = await connectStdio({
  command: 'npx',
  args: ['-y', '@modelcontextprotocol/server-everything'],
  stderr: 'inherit',
});

try {
  const cli = await cliFromMcp(client);
  process.exitCode = await runCli(cli, process.argv.slice(2));
} finally {
  await client.close();
}
