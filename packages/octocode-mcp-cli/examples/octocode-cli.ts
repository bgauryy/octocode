import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cliFromMcp, connectClient, createStdioTransport, runCli } from 'octocode-mcp-cli';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const server = resolve(repoRoot, 'packages/octocode-mcp/dist/index.js');

if (!existsSync(server)) {
  process.stderr.write(`Missing MCP server: ${server}\n`);
  process.exitCode = 2;
} else {
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined) env[key] = value;
  }
  env.OCTOCODE_ENABLE_LOCAL = 'true';
  env.OCTOCODE_BETA = 'true';
  env.OCTOCODE_STORAGE_MODE = 'persistent';
  const transport = createStdioTransport({
    command: 'node',
    args: [server],
    cwd: repoRoot,
    env,
    stderr: 'pipe',
  });
  transport.stderr?.on('data', () => {});
  const client = await connectClient(transport);
  try {
    const cli = await cliFromMcp(client);
    process.exitCode = await runCli(cli, process.argv.slice(2));
  } finally {
    await client.close();
  }
}
