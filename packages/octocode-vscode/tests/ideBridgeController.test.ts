import * as fs from 'fs/promises';
import * as path from 'path';
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('vscode', async () => (await import('./ideBridgeMock')).vscodeMock());

import { IdeBridgeController } from '../src/ideBridgeController';
import { ctx, useBridge } from './ideBridgeHarness';
import { host } from './ideBridgeMock';

useBridge();

describe('IdeBridgeController', () => {
  let controller: IdeBridgeController;
  const file = () => path.join(ctx.home, 'ide', `${process.pid}.json`);

  beforeEach(() => {
    controller = new IdeBridgeController({
      home: ctx.home,
      output: line => ctx.log.push(line),
    });
  });
  afterEach(() => controller.stop());

  it('starts when the setting is true or unset, and not when false', async () => {
    host.enabled = false;
    await controller.activate();
    await expect(fs.stat(file())).rejects.toMatchObject({ code: 'ENOENT' });
    await controller.stop();

    host.enabled = undefined;
    controller = new IdeBridgeController({
      home: ctx.home,
      output: line => ctx.log.push(line),
    });
    await controller.activate();
    await fs.stat(file());
  });

  it('follows the setting when it toggles and ignores other settings', async () => {
    await controller.activate();
    await fs.stat(file());

    host.enabled = false;
    host.configChanged!({ affectsConfiguration: () => false });
    await new Promise(resolve => setTimeout(resolve, 20));
    await fs.stat(file());

    host.configChanged!({
      affectsConfiguration: key => key === 'octocode.ideBridge.enabled',
    });
    await vi.waitFor(() =>
      expect(fs.stat(file())).rejects.toMatchObject({ code: 'ENOENT' })
    );

    host.enabled = true;
    host.configChanged!({
      affectsConfiguration: key => key === 'octocode.ideBridge.enabled',
    });
    await vi.waitFor(() => fs.stat(file()));
  });

  it('writes failures to the output instead of throwing', async () => {
    await fs.writeFile(path.join(ctx.home, 'ide'), 'blocked');
    await expect(controller.activate()).resolves.toBeUndefined();
    expect(ctx.log.join('\n')).toContain('IDE bridge failed');
  });

  it('dispose removes the lock and the config listener', async () => {
    await controller.activate();
    controller.dispose();
    await controller.stop();
    await expect(fs.stat(file())).rejects.toMatchObject({ code: 'ENOENT' });
    expect(host.configDispose).toHaveBeenCalledTimes(1);
  });
});
