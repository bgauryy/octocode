import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { checkSkill, checkStartup, python, runtimeInfo } from './artifact-checks.mjs';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = join(root, '../octocode-config/python/octocode_config.py');
const destination = join(root, 'scripts/octocode_config.py');
// The vendored copy is refreshed only inside the monorepo; a standalone checkout keeps it.
if (existsSync(source) && (!existsSync(destination) || !readFileSync(source).equals(readFileSync(destination)))) {
  const temporary = destination + '.' + process.pid + '.tmp';
  try { copyFileSync(source, temporary); renameSync(temporary, destination); }
  finally { rmSync(temporary, { force: true }); }
}
const runtime = runtimeInfo();
// Compile source in memory: building the portable runtime creates no bytecode artifacts.
execFileSync(python(), ['-B', '-c', 'import pathlib,sys; root=pathlib.Path(sys.argv[1]); [compile(p.read_bytes(),str(p),"exec") for p in root.rglob("*.py")]', join(root, 'scripts')], { stdio: 'inherit', timeout: 10000 });
if (process.platform !== 'win32') for (const name of ['communication.py', 'agents-communication', 'inbox-hook']) {
  const path = join(root, 'scripts', name); if (existsSync(path)) chmodSync(path, 0o755);
}
const executable = join(root, 'scripts/communication.py');
console.log(JSON.stringify({ executable, runtime, startup: checkStartup(executable), skill: checkSkill(executable, join(root, 'OPERATING.md')) }));

await build({ entryPoints: [join(root, 'src/interfaces/mcp.mjs'), join(root, 'src/interfaces/cli.mjs')], outdir: join(root, 'dist'), bundle: true, banner: { js: "import { createRequire as __createRequire } from 'node:module'; const require = __createRequire(import.meta.url);" }, platform: 'node', format: 'esm', target: 'node24', alias: { 'octocode-mcp-cli': join(root, '../octocode-mcp-cli/src/index.ts') } });
