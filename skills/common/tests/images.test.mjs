import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildReport } from '../scripts/lib/report/render.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'report-images-'));
  t.after(() => rmSync(root, {recursive:true,force:true}));
  mkdirSync(join(root,'nested'));
  return root;
}
test('nested report images embed bytes independent of HTML destination; code examples are not assets', t => {
  const root=fixture(t), bytes=Buffer.from('example image bytes');
  writeFileSync(join(root,'nested','image space.png'),bytes);
  writeFileSync(join(root,'nested','report.md'),'# Images\n\n![Evidence](image%20space.png)\n\n```md\n![example](not-real.png)\n```');
  const result=buildReport(root,{workspace:root});
  assert.deepEqual(result.warnings,[]);
  assert.ok(result.html.includes('data:image/png;base64,'+bytes.toString('base64')));
  assert.ok(result.html.includes('image%20space.png'));
});
test('missing local images warn while remote URLs require no build fetch', t => {
  const root=fixture(t);
  writeFileSync(join(root,'report.md'),'# Images\n\n![Missing](gone.png)\n\n![Remote](https://example.com/image.png)');
  const result=buildReport(join(root,'report.md'),{workspace:root});
  assert.equal(result.warnings.length,1);
  assert.match(result.warnings[0],/image gone.png/);
});
test('image symlinks cannot embed files outside the workspace', t => {
  const root=fixture(t), outside=fixture(t);
  writeFileSync(join(outside,'secret.png'),'outside');
  symlinkSync(join(outside,'secret.png'),join(root,'link.png'));
  writeFileSync(join(root,'report.md'),'# Image\n\n![Linked](link.png)');
  const result=buildReport(join(root,'report.md'),{workspace:root});
  assert.match(result.warnings[0],/inside the workspace/);
  assert.ok(!result.html.includes('data:image/png;base64,'+Buffer.from('outside').toString('base64')));
});
