import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { getSessionDir, getScreenshotDir } from '../src/chrome-debug.js';
import { getInternalErrorLogPath } from '../src/index.js';
import { checkpointStoreDir } from '../src/tools/checkpoints.js';
import { getDiscoveryFilePath } from '../src/tools/discovery-file.js';
import { getRegistryDir } from '../src/tools/dynamic-tools.js';
import { projectMcpPath } from '../src/tools/mcp-config.js';
import { createSessionArtifactContext } from '../src/tools/session-artifacts.js';

const previousOctocodeHome = process.env.OCTOCODE_HOME;

afterEach(() => {
  if (previousOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = previousOctocodeHome;
});

describe('global-only extension-owned paths', () => {
  it('keeps session, discovery, MCP, browser, checkpoint, dynamic-tool, and log artifacts under OCTOCODE_HOME/extension', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-global-paths-'));
    const workspace = path.join(root, 'workspace');
    const octocodeHome = path.join(root, 'octocode-home');
    fs.mkdirSync(workspace);
    process.env.OCTOCODE_HOME = octocodeHome;

    const session = { getSessionId: () => 'session-1' };
    const context = createSessionArtifactContext({ cwd: workspace, sessionManager: session });
    const paths = [
      context.root,
      getDiscoveryFilePath(workspace),
      projectMcpPath(workspace),
      getSessionDir(workspace, 9222, context.identity.sessionKey),
      getScreenshotDir(workspace, context.identity.sessionKey),
      checkpointStoreDir(workspace, octocodeHome),
      getRegistryDir({ ...process.env, OCTOCODE_HOME: octocodeHome }),
      getInternalErrorLogPath(workspace, session),
    ];
    const agentRoot = path.join(octocodeHome, 'extension');

    for (const candidate of paths) {
      expect(path.relative(agentRoot, candidate)).not.toMatch(/^\.\.(?:\/|$)/u);
      expect(path.relative(workspace, candidate)).toMatch(/^\.\.(?:\/|$)/u);
    }
  });
});
