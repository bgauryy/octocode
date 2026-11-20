// page-pdf against a real headless Chrome: the file is a PDF and options are validated.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { skip, startHarness } from './coverage-fixture.mjs';

test(
  'page-pdf writes a PDF of the page, honors options and refuses a bad scale',
  { skip, timeout: 180000 },
  async () => {
    const h = await startHarness();
    try {
      assert.equal(h.opened.ok, true);
      await h.cli('open', { port: h.port, url: `${h.origin}/` });
      const run = await h.check('page-pdf', {
        PDF_LANDSCAPE: true,
        PDF_PRINT_BACKGROUND: true,
        PDF_SCALE: 0.5,
      });
      assert.equal(run.out.ok, true, run.stdout);
      const pdf = readFileSync(run.artifacts.PAGE_PDF);
      assert.equal(pdf.subarray(0, 5).toString('latin1'), '%PDF-');
      assert.match(run.stdout, new RegExp(`bytes=${pdf.length} `));
      assert.match(run.stdout, /landscape=true printBackground=true scale=0.5/);

      const bad = await h.check('page-pdf', { PDF_SCALE: 5 });
      assert.notEqual(bad.out.ok, true);
    } finally {
      await h.stop();
    }
  }
);
