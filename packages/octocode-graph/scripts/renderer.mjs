import { build } from 'esbuild';
import { readFile, readdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';

export async function buildRenderer(root, update) {
  const assets = join(root, 'skills/octocode-graph/assets');
  const result = await build({
    absWorkingDir: root,
    entryPoints: ['src/renderer.ts'],
    bundle: true,
    platform: 'browser',
    format: 'esm',
    target: 'es2022',
    minify: true,
    outfile: join(assets, 'renderer.generated.js'),
    metafile: true,
    write: false,
    legalComments: 'eof',
    banner: {
      js: '/*! Third-party licenses: renderer.generated.LICENSE.txt */',
    },
  });
  const packages = new Map();
  for (const path of Object.keys(result.metafile.inputs)) {
    const match = /^(.*node_modules\/((?:@[^/]+\/)?[^/]+))\//.exec(path);
    if (match) packages.set(resolve(root, match[1]), match[2]);
  }
  const notices = [];
  for (const [dir, name] of packages) {
    const pkg = JSON.parse(await readFile(join(dir, 'package.json'), 'utf8'));
    const texts = (await readdir(dir))
      .filter(file => /^(?:licen[sc]e|copying|notice)(?:$|[.\-_])/i.test(file))
      .sort();
    if (!texts.length) throw new Error(`Missing bundled license for ${name}`);
    for (const file of texts) {
      notices.push(
        `${name} ${pkg.version} — ${file}\n${await readFile(join(dir, file), 'utf8')}`
      );
    }
  }
  await update(
    join(assets, 'renderer.generated.LICENSE.txt'),
    Buffer.from(notices.sort().join('\n\n'))
  );
  for (const file of result.outputFiles) await update(file.path, file.contents);
}
