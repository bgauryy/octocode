import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import path from 'node:path';

/** An isolated source fixture for the real stdio acceptance run. */
export async function createLocalAcceptanceFixture(parent) {
  await mkdir(parent, { recursive: true });
  const root = await mkdtemp(path.join(parent, 'mcp-acceptance-'));
  await writeFile(
    path.join(root, 'math.ts'),
    '// Arithmetic fixture.\nexport function add(left: number, right: number) { return left + right; }\n'
  );
  await writeFile(
    path.join(root, 'entry.ts'),
    'import { add } from "./math.js";\nexport function double(value: number) {\n  // Observed call anchor.\n  return add(value, value);\n}\n'
  );
  await writeFile(
    path.join(root, 'other.ts'),
    'import { add } from "./math.js";\nexport const answer = add(20, 22);\n'
  );
  await writeFile(
    path.join(root, 'tsconfig.json'),
    JSON.stringify({
      compilerOptions: { target: 'ES2022', module: 'NodeNext', strict: true },
      include: ['*.ts'],
    })
  );
  return root;
}
