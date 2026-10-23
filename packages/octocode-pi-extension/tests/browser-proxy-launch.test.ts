import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { accepts, cleanUp, cleanups, connectVia, FORBIDDEN, start } from './browser-proxy-harness.js';

/** chrome-launcher's `launch`, as Octocode calls it: every launch names its profile folder. */
const launch = vi.hoisted(() => vi.fn<(options: { startingUrl: string; userDataDir: string; chromeFlags: string[] }) => Promise<{ port: number; kill: () => Promise<undefined> }>>());
vi.mock('chrome-launcher', () => ({ launch }));

const { launchChrome, proxyFlags, restrictWebRtc } = await import('../src/browser/launch.js');

afterEach(async () => {
  vi.unstubAllEnvs();
  launch.mockReset();
  await cleanUp();
});

const fakeChrome = () => {
  const kill = vi.fn(async () => undefined);
  launch.mockResolvedValue({ port: 9222, kill });
  return kill;
};
const isolatedHome = () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-proxy-home-'));
  vi.stubEnv('OCTOCODE_HOME', home);
  cleanups.push(() => fs.rmSync(home, { recursive: true, force: true }));
};

describe('launching Chrome behind the proxy', () => {
  it('names the proxy and removes the loopback bypass, so loopback is filtered too', async () => {
    const proxy = await start();
    expect(proxyFlags(proxy)).toEqual([`--proxy-server=http://127.0.0.1:${proxy.port}`, '--proxy-bypass-list=<-loopback>']);
  });

  it('starts the proxy with a fresh Chrome and stops it when Chrome is killed', async () => {
    isolatedHome();
    const kill = fakeChrome();
    const onBlocked = vi.fn();
    const chrome = await launchChrome(true, undefined, { env: {}, onBlocked });
    const flags = launch.mock.calls[0][0].chromeFlags;
    expect(flags).toEqual(expect.arrayContaining(['--headless=new', `--proxy-server=${chrome.proxy!.url}`, '--proxy-bypass-list=<-loopback>']));
    expect((await connectVia(chrome.proxy!, '127.0.0.1:80')).status).toBe(FORBIDDEN);
    expect(onBlocked).toHaveBeenCalledExactlyOnceWith({ host: '127.0.0.1:80', reason: 'private' });
    const { port } = chrome.proxy!;
    await chrome.kill();
    expect(kill).toHaveBeenCalledTimes(1);
    expect(await accepts(port)).toBe(false);
  });

  it('stops the proxy when Chrome fails to start', async () => {
    isolatedHome();
    let url = '';
    launch.mockImplementation(async (options: { chromeFlags: string[] }) => {
      url = options.chromeFlags.find((flag) => flag.startsWith('--proxy-server='))!.slice('--proxy-server='.length);
      throw new Error('no chrome');
    });
    await expect(launchChrome(true, undefined, { env: {} })).rejects.toThrow('no chrome');
    expect(await accepts(Number(new URL(url).port))).toBe(false);
  });

  it('does not proxy a Chrome on the persistent profile, which outlives the session, but still restricts its WebRTC', async () => {
    fakeChrome();
    const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-profile-'));
    cleanups.push(() => fs.rmSync(profile, { recursive: true, force: true }));
    const chrome = await launchChrome(false, profile);
    expect(launch.mock.calls[0][0].chromeFlags.join(' ')).not.toContain('--proxy');
    expect(chrome.proxy).toBeUndefined();
    expect(JSON.parse(fs.readFileSync(path.join(profile, 'Default', 'Preferences'), 'utf8'))).toEqual({ webrtc: { ip_handling_policy: 'disable_non_proxied_udp' } });
  });

  it('writes the WebRTC policy into a fresh profile before Chrome starts, not as a switch Chrome ignores', async () => {
    isolatedHome();
    let seen: unknown;
    launch.mockImplementation(async (options: { userDataDir: string; chromeFlags: string[] }) => {
      seen = JSON.parse(fs.readFileSync(path.join(options.userDataDir, 'Default', 'Preferences'), 'utf8'));
      expect(options.chromeFlags.join(' ')).not.toContain('webrtc');
      return { port: 9222, kill: async () => undefined };
    });
    await (await launchChrome(true, undefined, { env: {} })).kill();
    expect(seen).toEqual({ webrtc: { ip_handling_policy: 'disable_non_proxied_udp' } });
  });

  it('keeps the other preferences of an existing profile, replaces a weaker policy, and rebuilds an unreadable file', () => {
    const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-profile-'));
    cleanups.push(() => fs.rmSync(profile, { recursive: true, force: true }));
    const file = path.join(profile, 'Default', 'Preferences');
    fs.mkdirSync(path.dirname(file));
    fs.writeFileSync(file, JSON.stringify({ homepage: 'https://example.test', webrtc: { ip_handling_policy: 'default', multiple_routes_enabled: false } }));
    restrictWebRtc(profile);
    expect(JSON.parse(fs.readFileSync(file, 'utf8'))).toEqual({ homepage: 'https://example.test', webrtc: { ip_handling_policy: 'disable_non_proxied_udp', multiple_routes_enabled: false } });
    fs.writeFileSync(file, '{not json');
    restrictWebRtc(profile);
    expect(JSON.parse(fs.readFileSync(file, 'utf8'))).toEqual({ webrtc: { ip_handling_policy: 'disable_non_proxied_udp' } });
  });

  it('does not start Chrome when the policy cannot be written', async () => {
    fakeChrome();
    const blocker = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-profile-'));
    cleanups.push(() => fs.rmSync(blocker, { recursive: true, force: true }));
    const profile = path.join(blocker, 'file');
    fs.writeFileSync(profile, 'not a folder');
    await expect(launchChrome(false, profile)).rejects.toThrow();
    expect(launch).not.toHaveBeenCalled();
  });
});
