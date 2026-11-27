import { cliFromMcp, connectStdio, runCli } from 'octocode-mcp-cli';

// The first argument is the only directory the server may access.
// https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem
const allowed = process.argv[2];
if (allowed === undefined || allowed.startsWith('-')) {
  process.stderr.write('usage: filesystem-cli.ts <allowed-directory> [command] [flags]\n');
  process.exitCode = 2;
} else {
  const client = await connectStdio({
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-filesystem', allowed],
    stderr: 'inherit',
  });
  try {
    const cli = await cliFromMcp(client);
    process.exitCode = await runCli(cli, process.argv.slice(3));
  } finally {
    await client.close();
  }
}
