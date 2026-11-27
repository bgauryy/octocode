// The affiliate check passes only when the tab's own host is the affiliate site, not when the
// name merely appears somewhere in another site's URL.
import test from 'node:test';
import assert from 'node:assert/strict';
import { onAffiliateSite } from '../../dist/engine/cdp-checks/affiliates-stealth-check.mjs';

test('the affiliate landing is decided by the parsed host', () => {
  for (const href of [
    'https://affiliates.walmart.com/',
    'https://affiliates.walmart.com/program?ref=1#join',
    'https://AFFILIATES.walmart.com:443/',
  ])
    assert.equal(onAffiliateSite(href), true, href);
  for (const href of [
    'https://evil.test/?next=affiliates.walmart.com',
    'https://evil.test/affiliates.walmart.com/',
    'https://affiliates.walmart.com.evil.test/',
    'https://evil.test/#affiliates.walmart.com',
    'https://user:affiliates.walmart.com@evil.test/',
    'affiliates.walmart.com',
    'about:blank',
    '',
    undefined,
  ])
    assert.equal(onAffiliateSite(href), false, String(href));
});
