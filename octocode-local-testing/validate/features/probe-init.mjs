import { startServer } from '../../../harness/mcp-client.mjs';
import fs from 'node:fs';
const s = await startServer();
fs.writeFileSync(new URL('./init.json', import.meta.url), JSON.stringify({init: s.init, tools: s.tools}, null, 2));
console.log(Object.keys(s.init), s.tools.map(t=>t.name).join(','));
console.log(s.init.instructions?.length);
s.close();
