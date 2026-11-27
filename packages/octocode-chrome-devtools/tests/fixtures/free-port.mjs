// One free loopback TCP port for tests that start a browser or server on a chosen port.
import { createServer } from 'node:net';

/** A free port on 127.0.0.1 as a number (the OS picks it; it is released before this resolves). */
export const freePort = () =>
  new Promise((resolve, reject) => {
    const server = createServer().listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });

/** The same port as text, for argv. */
export const freePortText = async () => String(await freePort());
