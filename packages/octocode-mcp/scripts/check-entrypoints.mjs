// npm otherwise accepts an incomplete dist directory and silently omits a missing entry.
import { statSync } from 'node:fs';
import manifest from '../package.json' with { type: 'json' };

const path = new URL(manifest.bin, new URL('../', import.meta.url));
if (!statSync(path, { throwIfNoEntry: false })?.isFile()) {
  throw new Error(`Build octocode-mcp before packing: missing ${manifest.bin}`);
}
