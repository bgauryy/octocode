import { describe, expect, it, vi } from 'vitest';
import { PRIVATE_HOSTS } from './browser-network-harness.js';
import { PagePolicy } from '../src/browser/network.js';
import type { HostLookup } from '../src/web/guard.js';

const lookup = (table: Record<string, string[]>) =>
  vi.fn(async (name: string) => {
    const addresses = table[name];
    if (!addresses) throw new Error(`ENOTFOUND ${name}`);
    return addresses.map((address) => ({ address }));
  });
const policy = (table: Record<string, string[]> = {}, now = () => 0) => new PagePolicy({ report: () => undefined, lookup: lookup(table), now });

describe('PagePolicy', () => {
  it('lets a page reach public hosts and refuses private and unresolved ones without asking', async () => {
    const page = policy({ 'cdn.example': ['93.184.216.34'], 'rebind.example': ['93.184.216.34', '10.0.0.5'] });
    await expect(page.check('https://cdn.example/app.js', 'Script')).resolves.toBeUndefined();
    await expect(page.check('https://93.184.216.34/x', 'Image')).resolves.toBeUndefined();
    for (const url of PRIVATE_HOSTS) await expect(page.check(url, 'XHR')).rejects.toThrow(/^Blocked private or local address: /);
    // One private answer among public ones is private.
    await expect(page.check('https://rebind.example/', 'Document')).rejects.toThrow(/Blocked private or local address: rebind\.example/);
    await expect(page.check('https://nowhere.example/', 'Image')).rejects.toThrow(/Blocked unresolved host: nowhere\.example/);
    // Numeric spellings are normalized by the URL parser before the check.
    await expect(page.check('http://0x7f.1/', 'Image')).rejects.toThrow(/Blocked private/);
    await expect(page.check('http://2130706433/', 'Image')).rejects.toThrow(/Blocked private/);
    await expect(page.check('not a url', 'Image')).rejects.toThrow(/Not a URL/);
  });

  it('lets data: and blob: subresources through, and refuses a document that is not http(s)', async () => {
    const page = policy();
    await expect(page.check('data:text/plain,hi', 'Image')).resolves.toBeUndefined();
    await expect(page.check('blob:https://example.com/id', 'Script')).resolves.toBeUndefined();
    await expect(page.check('file:///etc/passwd', 'Document')).rejects.toThrow(/http\(s\) URLs only, not file:/);
    await expect(page.check('javascript:alert(1)', 'Document')).rejects.toThrow(/http\(s\) URLs only/);
  });

  it('reuses a host verdict for a short time, then asks the resolver again', async () => {
    let clock = 0;
    const resolver = lookup({ 'cdn.example': ['93.184.216.34'] });
    const page = new PagePolicy({ report: () => undefined, lookup: resolver, now: () => clock });
    await Promise.all([page.check('https://cdn.example/a', 'Script'), page.check('https://cdn.example/b', 'Image'), page.check('https://cdn.example/c', 'XHR')]);
    await page.check('https://cdn.example/d', 'Image');
    expect(resolver).toHaveBeenCalledTimes(1);
    clock = 31_000;
    await page.check('https://cdn.example/e', 'Image');
    expect(resolver).toHaveBeenCalledTimes(2);
  });
});

describe('PagePolicy approvals', () => {
  it('approves only the host and port the agent opened, decides nothing itself, and forgets them on revoke', async () => {
    const resolver = lookup({ 'cdn.example': ['93.184.216.34'], 'app.local': ['127.0.0.1'] });
    const page = new PagePolicy({ report: () => undefined, lookup: resolver });
    page.approve(new URL('http://127.0.0.1:3000/app'), 'session');
    page.approve(new URL('http://app.local:8080/'), 'session');
    // Approval never resolves a name: the caller classified once and asked net.private.
    expect(resolver).not.toHaveBeenCalled();
    await expect(page.check('https://cdn.example/x', 'Script')).resolves.toBeUndefined();
    await expect(page.check('http://127.0.0.1:3000/app.js', 'Script')).resolves.toBeUndefined();
    await expect(page.check('http://app.local:8080/x', 'XHR')).resolves.toBeUndefined();
    // Another port, another host of the same machine: still the page's own choice.
    await expect(page.check('http://127.0.0.1:3001/', 'XHR')).rejects.toThrow(/Blocked private/);
    await expect(page.check('http://app.local/', 'Document')).rejects.toThrow(/Blocked private/);
    await expect(page.check('http://10.0.0.1:3000/', 'Image')).rejects.toThrow(/Blocked private/);
    page.revoke();
    await expect(page.check('http://127.0.0.1:3000/app.js', 'Script')).rejects.toThrow(/Blocked private/);
    await expect(page.check('http://app.local:8080/x', 'XHR')).rejects.toThrow(/Blocked private/);
  });

  it('approves a once-approved host for its tab only, until that tab leaves the host or closes', async () => {
    const page = policy();
    page.approve(new URL('http://127.0.0.1:3000/'), { tab: 'a' });
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'a' })).resolves.toBeUndefined();
    // Another tab, or a request with no tab behind it, never reaches it; nor does another port.
    await expect(page.check('http://127.0.0.1:3000/x.js', 'XHR', { tab: 'b' })).rejects.toThrow(/Blocked private/);
    await expect(page.check('http://127.0.0.1:3000/x.js', 'XHR')).rejects.toThrow(/Blocked private/);
    await expect(page.check('http://127.0.0.1:3001/x.js', 'XHR', { tab: 'a' })).rejects.toThrow(/Blocked private/);
    // The same host, a path or fragment away, is still its own.
    page.navigated('a', 'http://127.0.0.1:3000/other#frag');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'a' })).resolves.toBeUndefined();
    page.navigated('b', 'https://example.com/');
    // Navigating to another host ends it, though the tab comes back.
    page.navigated('a', 'https://93.184.216.34/');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'a' })).rejects.toThrow(/Blocked private/);
    page.navigated('a', 'http://127.0.0.1:3000/');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'a' })).rejects.toThrow(/Blocked private/);
    // A tab that closes takes its approval along.
    page.approve(new URL('http://127.0.0.1:3000/'), { tab: 'a' });
    expect(page.approves('127.0.0.1', 3000)).toBe(true);
    page.closed('a');
    expect(page.approves('127.0.0.1', 3000)).toBe(false);
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'a' })).rejects.toThrow(/Blocked private/);
    // A tab approved for one host moves to the next one the agent opens.
    page.approve(new URL('http://127.0.0.1:3000/'), { tab: 'a' });
    page.approve(new URL('http://127.0.0.1:4000/'), { tab: 'a' });
    expect(page.approves('127.0.0.1', 3000)).toBe(false);
    expect(page.approves('127.0.0.1', 4000)).toBe(true);
  });

  it('gives a popup the approval of the tab that opened it, until the popup leaves the host', async () => {
    const page = policy();
    page.approve(new URL('http://127.0.0.1:3000/'), { tab: 'a' });
    page.opened('a', 'popup');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'popup' })).resolves.toBeUndefined();
    page.navigated('popup', 'https://93.184.216.34/');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'popup' })).rejects.toThrow(/Blocked private/);
    // The opener keeps its own; a popup of a tab with none gets none.
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'a' })).resolves.toBeUndefined();
    page.opened('other', 'popup2');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'Script', { tab: 'popup2' })).rejects.toThrow(/Blocked private/);
  });

  it('keeps a session approval for every tab, whatever they navigate to, until revoke', async () => {
    const page = policy();
    page.approve(new URL('http://127.0.0.1:3000/'), 'session');
    for (const tab of ['a', 'b', undefined]) await expect(page.check('http://127.0.0.1:3000/x.js', 'XHR', tab ? { tab } : {})).resolves.toBeUndefined();
    page.navigated('a', 'https://93.184.216.34/');
    await expect(page.check('http://127.0.0.1:3000/x.js', 'XHR', { tab: 'a' })).resolves.toBeUndefined();
    expect(page.approves('127.0.0.1', 3000)).toBe(true);
    page.revoke();
    await expect(page.check('http://127.0.0.1:3000/x.js', 'XHR', { tab: 'a' })).rejects.toThrow(/Blocked private/);
  });
});

describe('PagePolicy workers and DNS rebinding', () => {
  it('lets a worker of an approved origin reach that host while a tab holds it, and no worker of another origin', async () => {
    const page = policy();
    page.approve(new URL('http://127.0.0.1:3000/'), { tab: 'a' });
    await expect(page.check('http://127.0.0.1:3000/data', 'Fetch', { origin: 'http://127.0.0.1:3000' })).resolves.toBeUndefined();
    await expect(page.check('http://127.0.0.1:3000/data', 'Fetch', { origin: 'https://evil.example' })).rejects.toThrow(/Blocked private/);
    page.closed('a');
    await expect(page.check('http://127.0.0.1:3000/data', 'Fetch', { origin: 'http://127.0.0.1:3000' })).rejects.toThrow(/Blocked private/);
  });

  it('refuses a name that answered public once and private since, and sees it answer from a private address', async () => {
    let clock = 0;
    let answers = 0;
    const flipping: HostLookup = async () => [{ address: answers++ === 0 ? '93.184.216.34' : '10.0.0.5' }];
    const page = new PagePolicy({ report: () => undefined, lookup: flipping, now: () => clock });
    await expect(page.check('https://rebind.example/', 'Document')).resolves.toBeUndefined();
    clock = 31_000;
    await expect(page.check('https://rebind.example/x', 'XHR')).rejects.toThrow(/Blocked private or local address: rebind\.example/);
    expect(page.rebinds('https://rebind.example/x', '10.0.0.5', 443)).toBe(true);
  });

  it('sees a public name answered from a private address, but not an approved or proxied one', async () => {
    const page = policy({ 'app.local': ['127.0.0.1'] });
    page.approve(new URL('http://app.local:8080/'), { tab: 'tab-1' });
    expect(page.rebinds('https://cdn.example/x', '10.0.0.5', 443)).toBe(true);
    expect(page.rebinds('https://cdn.example/x', '[::1]', 443)).toBe(true);
    expect(page.rebinds('https://cdn.example/x', '93.184.216.34', 443)).toBe(false);
    // Chrome connected to another port than the URL's: a proxy, whose address says nothing.
    expect(page.rebinds('https://cdn.example/x', '127.0.0.1', 3128)).toBe(false);
    expect(page.rebinds('http://app.local:8080/x', '127.0.0.1', 8080)).toBe(false);
    // A literal address was already judged by `check`.
    expect(page.rebinds('http://127.0.0.1:3000/', '127.0.0.1', 3000)).toBe(false);
    expect(page.rebinds('data:text/plain,x', '127.0.0.1')).toBe(false);
  });
});
