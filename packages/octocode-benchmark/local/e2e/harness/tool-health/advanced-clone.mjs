// ghCloneRepo against the mock's git smart-HTTP repository: sparse paths and history
// depth each name their own checkout, and a refresh never overwrites a local edit.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const REPO = { owner: 'octo', repo: 'demo' };
const data = result => result?.json?.results?.[0]?.data ?? {};
const count = cwd => Number(spawnSync('git', ['rev-list', '--count', 'HEAD'], { cwd, encoding: 'utf8' }).stdout?.trim());
const sparseFiles = (location, included, excluded) => !!location && fs.existsSync(path.join(location, included)) && !fs.existsSync(path.join(location, excluded));

export async function clone(ctx) {
  const call = async query => data(await ctx.call(ctx.q({ ...REPO, ...query }), 'cli'));
  const first = await call({ paths: ['src'], historyDepth: 1 });
  const second = await call({ paths: ['docs'], historyDepth: 1 });
  const deep = await call({ paths: ['src'], historyDepth: 2 });
  const a = first.location?.localPath, b = second.location?.localPath, c = deep.location?.localPath;
  const out = [
    ['advanced.clone.sparseIdentity', a !== b && sparseFiles(a, 'src/greeter.ts', 'docs/notes.md') && sparseFiles(b, 'docs/notes.md', 'src/greeter.ts'), JSON.stringify({ first, second })],
    ['advanced.clone.depthIdentity', !!a && !!c && a !== c && count(a) === 1 && count(c) === 2, JSON.stringify(deep)],
  ];
  if (a) {
    const file = path.join(a, 'src/greeter.ts');
    fs.appendFileSync(file, '\n// local edit retained\n');
    const refreshed = await call({ paths: ['src'], historyDepth: 1 });
    out.push(['advanced.clone.preserveLocalEdits', refreshed.errorCode === 'checkoutDirty' && fs.readFileSync(file, 'utf8').endsWith('// local edit retained\n'), JSON.stringify(refreshed)]);
  }
  return out;
}
