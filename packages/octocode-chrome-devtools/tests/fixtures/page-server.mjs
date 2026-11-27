// A loopback HTML server child that exits with its parent. The HTML travels as data in argv,
// never spliced into code; the child prints its port as the first stdout chunk.
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { DIE_WITH_PARENT } from './die-with-parent.mjs';

const server = fileURLToPath(new URL('./page-server.cjs', import.meta.url));

/** Serves `routes[url]` for a url it names and `page` for every other one, as text/html. */
export const spawnPageServer = (page, routes = {}) =>
  spawn(
    process.execPath,
    [...DIE_WITH_PARENT, server, JSON.stringify({ page, routes })],
    { stdio: ['pipe', 'pipe', 'inherit'] }
  );
