// A small project with known import, call, inheritance and argument edges, plus the
// shapes the graph tools classify as unresolved, for graphBuild and graphQuery.
import fs from 'node:fs';
import path from 'node:path';

const FILES = {
  'package.json': '{ "name": "matrix-graph", "version": "1.0.0", "dependencies": { "declared-dep": "^1.0.0" } }\n',
  'src/a.ts': [
    "import { b } from './b';",
    "import { gone } from './missing';",
    "import { pick } from 'undeclared-pkg';",
    "import { use } from 'declared-dep';",
    'export class Base { run() { return 1; } }',
    'export class Child extends Base { run() { return b(); } }',
    'export function a(cb: () => void) { b(); pick(); use(); gone(); cb(); unknownGlobal(); }',
    'export function pass() { a(b); }',
    'export function member(x: { run(): number }) { return x.run(); }',
    'export function spread(args: []) { a(...args); }',
    'export function make() { return new Missing(); }',
    '',
  ].join('\n'),
  'src/b.ts': "import { c } from './c';\nexport function b() { return c(); }\n",
  'src/c.ts': "import { a } from './a';\nexport function c() { a(() => {}); return 1; }\n",
  'src/orphan.ts': 'export function neverUsed() { return 2; }\nexport function localOnly() { return 3; }\nlocalOnly();\n',
  'src/helper.ts': 'export function testOnly() { return 4; }\n',
  'src/same-one.ts': 'export function twin() { return 1; }\n',
  'src/same-two.ts': 'export function twin() { return 2; }\n',
  'src/caller.ts': "import * as one from './same-one';\nexport function callsTwin() { twin(); return one; }\n",
  'test/a.test.ts': "import { a } from '../src/a';\nimport { testOnly } from '../src/helper';\na(() => {});\ntestOnly();\n",
};

export function buildGraphWorld(dir) {
  for (const [file, text] of Object.entries(FILES)) {
    const target = path.join(dir, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, text);
  }
}
