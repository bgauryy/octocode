import { mkdirSync } from 'node:fs';
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
    label: 'Octocode Claude Code plugin',
    writeHostFiles({ pkg, manifest, mcpServer }) {
      mkdirSync(join(packageRoot, '.claude-plugin'), { recursive: true });
      writeJson(join(packageRoot, '.claude-plugin/plugin.json'), {
        ...manifest,
        version: pkg.version,
      });
      writeJson(join(packageRoot, '.mcp.json'), {
        mcpServers: { octocode: mcpServer },
      });
    },
  });
}

if (isMainModule(import.meta.url)) build();
