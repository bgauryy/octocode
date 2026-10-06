import os from 'node:os';
import { describe, expect, it, vi } from 'vitest';
import { checkBrowserUrl, checkUploadPaths } from '../src/browser/tool.js';

describe('browser reach', () => {
  const ui = (answer: boolean) => {
    const confirm = vi.fn(async () => answer);
    return { hasUI: true, cwd: '/work', ui: { confirm } as never, confirm };
  };

  it('opens http(s) only, and private or local hosts only once the user confirms them', async () => {
    vi.stubEnv('OCTOCODE_WEB_ALLOW_PRIVATE', '');
    const allowed = new Set<string>();
    await expect(checkBrowserUrl('file:///etc/passwd', ui(true), allowed)).rejects.toThrow(/http\(s\) URLs only/);
    await expect(checkBrowserUrl('javascript:alert(1)', ui(true), allowed)).rejects.toThrow(/http\(s\) URLs only/);
    await expect(checkBrowserUrl('not a url', ui(true), allowed)).rejects.toThrow(/Not a URL/);
    await expect(checkBrowserUrl('https://93.184.216.34/', { hasUI: false, ui: {} as never }, allowed)).resolves.toBeUndefined();
    await expect(checkBrowserUrl('http://169.254.169.254/latest/meta-data', { hasUI: false, ui: {} as never }, allowed)).rejects.toThrow(/Blocked private or local address/);
    const declined = ui(false);
    await expect(checkBrowserUrl('http://127.0.0.1:3000/', declined, allowed)).rejects.toThrow(/confirmation/);
    const confirmed = ui(true);
    await checkBrowserUrl('http://127.0.0.1:3000/', confirmed, allowed);
    await checkBrowserUrl('http://127.0.0.1:3000/next', confirmed, allowed);
    expect(confirmed.confirm).toHaveBeenCalledTimes(1);
    vi.stubEnv('OCTOCODE_WEB_ALLOW_PRIVATE', '1');
    await expect(checkBrowserUrl('http://10.0.0.1/', { hasUI: false, ui: {} as never }, new Set())).resolves.toBeUndefined();
  });

  it('uploads files outside the workspace only with the user\'s confirmation', async () => {
    await expect(checkUploadPaths(['a.txt', '/work/sub/b.txt'], ui(false))).resolves.toBeUndefined();
    await expect(checkUploadPaths(['../secret', '/etc/passwd'], ui(false))).rejects.toThrow(/outside the workspace.*\/secret, \/etc\/passwd/);
    await expect(checkUploadPaths(['/etc/passwd'], { cwd: '/work', hasUI: false, ui: {} as never })).rejects.toThrow(/outside the workspace/);
    await expect(checkUploadPaths(['~/.ssh/id_rsa'.replace('~', os.homedir())], ui(true))).resolves.toBeUndefined();
  });
});
