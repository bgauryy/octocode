import { main } from '../runtime/cli.js';
import manifest from '../../package.json' with { type: 'json' };
export async function runCommunicationMcp(argv: readonly string[] = process.argv.slice(2)): Promise<number> {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(`@octocodeai/octocode-agents-communication — MCP stdio server (default)

Let the agent initialize its identity through the init tool:
  npx -y @octocodeai/octocode-agents-communication --workspace <repo>
Or connect an existing identity:
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
Requires Node 24.15+ (24.x).`);
    return 0;
  }
  if (argv.includes('--version')) { console.log(manifest.version); return 0; }
  return main(['mcp', ...argv]);
}
