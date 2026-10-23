import { describe, expect, it, vi } from 'vitest';

import { getOctocodeHome } from '../src/index.js';

describe('getOctocodeHome', () => {
  it('OCTOCODE_HOME override wins, path is resolved', () => {
    expect(getOctocodeHome({ OCTOCODE_HOME: '/custom/home' })).toBe(
      '/custom/home'
    );
  });

  it('trims whitespace from OCTOCODE_HOME override', () => {
    expect(getOctocodeHome({ OCTOCODE_HOME: '  /trimmed  ' })).toBe('/trimmed');
  });

  it('empty / blank OCTOCODE_HOME falls through to homedir default', () => {
    const def = getOctocodeHome({ OCTOCODE_HOME: '' });
    expect(def.endsWith('.octocode')).toBe(true);
  });

  it('whitespace-only OCTOCODE_HOME falls through to homedir default', () => {
    const def = getOctocodeHome({ OCTOCODE_HOME: '   ' });
    expect(def.endsWith('.octocode')).toBe(true);
  });

  it('default uses os.homedir()/.octocode on every platform', async () => {
    vi.resetModules();
    vi.doMock('node:os', () => ({
      homedir: () => '/home/test',
    }));

    const { getOctocodeHome: getMockedHome } = await import('../src/home.js');
    expect(getMockedHome({})).toBe('/home/test/.octocode');
    expect(
      getMockedHome({ XDG_CONFIG_HOME: '/xdg', APPDATA: 'D:\\Roaming' })
    ).toBe('/home/test/.octocode');

    vi.doUnmock('node:os');
    vi.resetModules();
  });

  it('no arguments uses process.env defaults without throwing', () => {
    expect(() => getOctocodeHome()).not.toThrow();
    expect(typeof getOctocodeHome()).toBe('string');
  });
});
