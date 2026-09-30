// Ad-hoc probe: node pr-large-probe.mjs '<json query>' [raw]
import {startServer} from './mcp-client.mjs';
const c=await startServer();
const q=JSON.parse(process.argv[2]);
const e=process.argv[3]==='raw'?await c.raw('ghGetHistoryItem',q):await c.call('ghGetHistoryItem',q);
console.log(`ms=${e.ms} chars=${e.text.length} isError=${e.isError}`);
console.log(process.argv[4]==='sc'?JSON.stringify(e.sc,null,1):e.text);
c.close();
