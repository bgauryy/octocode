import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';

/** Configure the protocol fixture, never an installed semantic language server. */
export function configureMockLsp(home) {
  const directory = path.join(home, '.octocode');
  fs.mkdirSync(directory, { recursive: true });
  const configPath = path.join(directory, 'lsp-servers.json');
  const trace = path.join(directory, 'mock-lsp.jsonl');
  const languages = { '.ts': 'typescript', '.tsx': 'typescriptreact', '.js': 'javascript', '.jsx': 'javascriptreact', '.mjs': 'javascript', '.cjs': 'javascript', '.rs': 'rust' };
  const languageServers = Object.fromEntries(Object.entries(languages).map(([extension, languageId]) => [extension, {
    command: process.execPath,
    args: [path.join(import.meta.dirname, 'mock-lsp.mjs')],
    languageId,
    env: { OCTOCODE_TEST_LSP_TRACE: trace },
  }]));
  fs.writeFileSync(configPath, JSON.stringify({ languageServers }));
  return { OCTOCODE_LSP_AUTO_INSTALL: 'off', OCTOCODE_LSP_PREWARM: 'off', OCTOCODE_TRUST_PROJECT_LSP_CONFIG: 'false' };
}

/** Stop only brokers inside this test's private temp directory. */
export async function stopTestBrokers(tmp, { filesystem = fs, connect = socketPath => net.createConnection(socketPath), deadlineMs = 1000 } = {}) {
  const directories = filesystem.readdirSync(tmp, { withFileTypes: true }).filter(entry => entry.isDirectory() && entry.name.startsWith('octocode-'));
  const sockets = directories.flatMap(entry => {
    const directory = path.join(tmp, entry.name);
    return filesystem.readdirSync(directory).map(name => path.join(directory, name)).filter(file => filesystem.statSync(file, { throwIfNoEntry: false })?.isSocket());
  });
  await Promise.all(sockets.map(socketPath => new Promise(resolve => {
    const connection = connect(socketPath);
    const deadline = setTimeout(() => { connection.destroy(); resolve(); }, deadlineMs);
    const finish = () => { clearTimeout(deadline); resolve(); };
    connection.on('connect', () => connection.end('{"stop":true}\n'));
    connection.on('error', finish);
    connection.on('close', finish);
  })));
}
