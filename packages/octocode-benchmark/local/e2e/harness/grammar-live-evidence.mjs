import fs from 'node:fs';
import path from 'node:path';
import { runProjectCommand } from './grammar-live-projects.mjs';

/** Retain provider identities separately from the native engine identity. */
export async function captureProviderVersions({ env, root, cwd }) {
  const commands = [
    ['go', ['version']], ['gopls', ['version']], ['java', ['-version']], ['dotnet', ['--version']],
    ['csharp-ls', ['--version']], ['metals', ['--version']], ['asm-lsp', ['version']],
    ['scala-cli', ['version']], ['sbt', ['--script-version']], ['basedpyright', ['--version']],
    ['rust-analyzer', ['--version']], ['clangd', ['--version']],
  ];
  const sink = { setup: [] };
  for (const [command, args] of commands) {
    try { await runProjectCommand(command, args, { env, cwd, report: sink }); }
    catch { /* Preserve the version probe failure; semantic availability is asserted separately. */ }
  }
  const packages = ['typescript', 'typescript-language-server', '@typescript/native-preview'].map(name => {
    const file = path.join(root, 'node_modules', name, 'package.json');
    return { name, file, version: fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')).version : null };
  });
  const brew = env.PATH.split(path.delimiter).map(directory => path.join(directory, 'brew')).find(file => fs.existsSync(file));
  if (brew) {
    try { await runProjectCommand(brew, ['list', '--versions', 'jdtls'], { env, cwd, report: sink }); }
    catch { /* Package provenance is recorded, including its command status. */ }
  }
  return { commands: sink.setup, packages };
}

/** Collect server-owned logs before the private fixture directory is removed. */
export function captureServerLogs(tmp, fixtures = []) {
  const files = fixtures.map(row => path.join(path.dirname(row.file), '.metals/metals.log'));
  const jdtls = path.join(tmp, 'home/lsp-workspaces/jdtls');
  if (fs.existsSync(jdtls)) for (const directory of fs.readdirSync(jdtls)) files.push(path.join(jdtls, directory, '.metadata/.log'));
  return [...new Set(files)].filter(file => fs.existsSync(file)).map(file => ({ file: path.relative(tmp, file), text: fs.readFileSync(file, 'utf8') }));
}
