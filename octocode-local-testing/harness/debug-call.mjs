// Replay one or more calls and print full text: node harness/debug-call.mjs '<tool>' '<query json>' [...]
import { startServer } from './mcp-client.mjs';
const client = await startServer();
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i += 2) {
  const e = await client.call(args[i], JSON.parse(args[i + 1]));
  console.log(`\n### ${args[i]} ${e.ms}ms ${e.bytes}B\n${e.text.slice(0, Number(process.env.MAX || 1500))}`);
}
client.close();
