import { execute } from './runtime.mjs';
export async function runCommunicationMcp(argv = process.argv.slice(2)) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(`@octocodeai/octocode-agents-communication — MCP stdio server (default)

Connect an existing identity:
  npx -y @octocodeai/octocode-agents-communication --session <id> --workspace <repo>
Or create a connection-owned identity:
  npx -y @octocodeai/octocode-agents-communication --managed --vendor generic --name <unique-name> --workspace <repo>

Optional: --database <sqlite-file>, --tools messaging|review|editing|<comma-separated-names>.
Managed identities and their leases expire when this connection closes.
Use an existing --session for identity and leases that survive a CLI invocation.

CLI operations and help:
  npx -y @octocodeai/octocode-agents-communication /cli --help
  npx -y @octocodeai/octocode-agents-communication /cli join --help
Programmatic exports: package root (MCP), package/cli (CLI).
Requires Node 24.15+ (24.x) and Python 3.9+.`);
    return 0;
  }
  if (argv.includes('--version')) { console.log('0.1.0'); return 0; }
  await execute(['mcp', ...argv], { stream: true });
  return 0;
}
