import { execFileSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { python } from './artifact-checks.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
for (const directory of ['scripts', 'tests']) {
  execFileSync(python(), ['-B', '-c', 'import pathlib,sys; [compile(p.read_bytes(),str(p),"exec") for p in pathlib.Path(sys.argv[1]).rglob("*.py")]', join(root, directory)], { stdio: 'inherit' });
}
function check(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory() && entry.name !== '__pycache__') check(path);
    else if (entry.isFile() && /\.(mjs|js)$/.test(entry.name)) execFileSync(process.execPath, ['--check', path], { stdio: 'inherit' });
  }
}
for (const directory of ['scripts', 'src', 'tests']) check(join(root, directory));
console.log('Python and JavaScript syntax checks passed.');
