import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { LSPClient } from '../../src/lsp/client.js';
import { getLanguageServerForFile } from '../../src/lsp/config.js';

// Explicit real-provider check: needs the bundled TypeScript language server.
describe.skipIf(process.env.OCTOCODE_LSP_INTEGRATION !== 'true')(
  'cold TypeScript semantic definitions',
  () => {
    it.each(['named', 'default', 'reexport', 'paths'])(
      'resolves a cold %s import through the provider without opening its target',
      async variant => {
        const root = await mkdtemp(
          path.join(os.tmpdir(), 'octocode-cold-definition-')
        );
        const source = path.join(root, 'source.ts');
        const target = path.join(root, 'target.ts');
        const configFile = path.join(root, 'tsconfig.json');
        await writeFile(
          target,
          variant === 'default'
            ? 'export default function actual() { return 1; }\n'
            : 'export function actual() { return 1; }\n'
        );
        await writeFile(
          path.join(root, 'barrel.ts'),
          'export { actual as renamed } from "./target.js";\n'
        );
        const specifier =
          variant === 'paths'
            ? '@fixture/target'
            : variant === 'reexport'
              ? './barrel.js'
              : './target.js';
        const importLine =
          variant === 'default'
            ? `import target from '${specifier}';`
            : `import {\n  ${variant === 'reexport' ? 'renamed' : 'actual'} as target,\n} from '${specifier}';`;
        const content = `${importLine}\ntarget();\n`;
        await writeFile(source, content);
        await writeFile(
          configFile,
          JSON.stringify({
            compilerOptions: {
              module: 'NodeNext',
              moduleResolution: 'NodeNext',
              paths: { '@fixture/*': ['./*'] },
            },
            include: ['*.ts'],
          })
        );
        const config = await getLanguageServerForFile(source, root);
        expect(config).not.toBeNull();
        const client = new LSPClient(config!);
        try {
          await client.start();
          const definition = await client.gotoDefinition(source, {
            line: importLine.split('\n').length,
            character: 0,
          });
          expect(definition).toHaveLength(1);
          expect(
            definition.map(item =>
              item.uri.startsWith('file:') ? fileURLToPath(item.uri) : item.uri
            )
          ).toEqual([target]);
          expect(definition[0]?.content).toContain('function actual');
        } finally {
          await client.stop();
          await rm(root, { recursive: true, force: true });
        }
      },
      30_000
    );
  }
);
