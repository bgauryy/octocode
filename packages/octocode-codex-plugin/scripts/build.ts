import { cpSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  buildPlugin,
  isMainModule,
  writeJson,
} from '../../octocode/scripts/build-plugin.mjs';

export const packageRoot = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..'
);
export const repoRoot = resolve(packageRoot, '../..');

export function build() {
  buildPlugin({
    packageRoot,
    label: 'Octocode plugin',
    writeHostFiles({ pkg, manifest, mcpServer }) {
      writeJson(join(packageRoot, 'plugin.json'), {
        ...manifest,
        version: pkg.version,
      });
      writeJson(join(packageRoot, 'mcp.json'), {
        $schema: 'https://agent-plugins.org/schemas/1.0.0/mcp.schema.json',
        mcpServers: { octocode: mcpServer },
      });
      mkdirSync(join(packageRoot, 'assets'), { recursive: true });
      cpSync(
        join(repoRoot, 'assets/logo.png'),
        join(packageRoot, 'assets/logo.png')
      );
    },
  });
}

if (isMainModule(import.meta.url)) build();
