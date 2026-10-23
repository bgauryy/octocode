// Self-test for the report viewer: Markdown engine, widgets, routes, collection, checks and manifest.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { mdKit } from './markdown.mjs';
import { KINDS, SCRIPT_SKILL_DIR, resolveRefs } from './workspace.mjs';
import { checkReport, contractFor, localDate } from './checks.mjs';
import { buildManifest, buildReport, defaultOut, linksIn } from './render.mjs';

/** entryFile: the render-report.mjs script, re-run through a symlinked skill folder. */
export function selfTest(entryFile) {
  const kit = mdKit();
  const cases = [];
  const check = (name, cond) => cases.push({ name, ok: Boolean(cond) });
  const html = kit.parse([
    '# Title', '', 'Lead with **bold**, *em*, `code`, ~~gone~~ and https://example.com/x.', '',
    '| Area | Severity | Note |', '|:--|:-:|--:|', '| a \\| b | High | `x|y` |', '',
    '- one', '  - nested', '- [x] done', '- [ ] open', '',
    '3. three', '4. four', '',
    '> [!WARNING]', '> Careful.', '', '> plain quote', '',
    'Ref [docs][d] and note[^n].', '', '[d]: https://docs.example.com "Docs"', '[^n]: The note.', '',
    '```mermaid', 'flowchart LR', '  A-->B', '```', '',
    '```ts', 'const s = "<b>"; // c', '```', '',
    '<script>alert(1)</script>', '', '[x](javascript:alert(1)) line<br>break', '',
    'Setext', '------', '', '<details>', '<summary>More</summary>', '', 'Hidden.', '', '</details>',
  ].join('\n'));
  check('heading', html.includes('<h1>Title</h1>'));
  const graphInput = {title:'Graph',nodes:[{id:'a',label:'A'},{id:'b',label:'B',url:'https://example.com'}],edges:[{source:'a',target:'b'}]};
  check('graph links appear in manifest', linksIn('```graph\n'+JSON.stringify(graphInput)+'\n```').includes('https://example.com'));
  check('graph data normalized', kit.widget('graph',JSON.stringify(graphInput)).edges[0].label === '');
  const chartInput = {title:'Chart',type:'bar',labels:['One'],series:[{name:'Count',values:[1]}]};
  check('chart data normalized', kit.widget('chart',JSON.stringify(chartInput)).unit === '');
  for (const [label,kind,data] of [
    ['duplicate nodes','graph',{...graphInput,nodes:[graphInput.nodes[0],graphInput.nodes[0]]}],
    ['missing endpoint','graph',{...graphInput,edges:[{source:'a',target:'missing'}]}],
    ['unsafe URL','graph',{...graphInput,nodes:[{id:'a',label:'A',url:'javascript:alert(1)'}]}],
    ['empty title','chart',{...chartInput,title:''}],
    ['mismatched series','chart',{...chartInput,labels:['One','Two']}],
    ['string number','chart',{...chartInput,series:[{name:'A',values:['1']}]}],
    ['unsupported chart','chart',{...chartInput,type:'custom'}],
    ['negative pie','chart',{...chartInput,type:'pie',series:[{name:'A',values:[-1]}]}],
    ['multiple pie series','chart',{...chartInput,type:'pie',series:[chartInput.series[0],chartInput.series[0]]}]
  ]) { let rejected=false; try { kit.widget(kind,JSON.stringify(data)); } catch { rejected=true; } check('rejects '+label,rejected); }
  check('widget markup escapes embedded HTML',kit.parse('```graph\n{"title":"<script>alert(1)</script>"}\n```').includes('&lt;script&gt;'));

  check('editor file paths cannot become URL fragments or queries', new URL('vscode://file' + kit.encodePath('/docs/a#b?c.md')).hash === '' && new URL('vscode://file' + kit.encodePath('/docs/a#b?c.md')).search === '');
  for (const p of ['guide@v2.md', 'My Guide.md', '日本語.md', 'a#b.md', '100%.md']) {
    for (const slug of ['', 'section-2']) {
      const parsed = kit.routeParse(kit.routeHref(p, slug));
      check(`route round trip: ${p} ${slug}`, parsed.path === p && parsed.slug === slug);
    }
  }
  check('manifest uses rendered links, ignores examples and unused definitions', JSON.stringify(linksIn('[API](https://example.com/api(v2)?a=1&b=2)\n\n```md\n[fake](https://fake.example)\n```\n\n[unused]: https://unused.example')) === JSON.stringify(['https://example.com/api(v2)?a=1&b=2']));

  check('angle links preserve spaces', kit.parse('[file](<docs/My Guide.md>)').includes('href="docs/My Guide.md"'));
  check('links preserve balanced parentheses', kit.parse('[API](https://example.com/api(v2))').includes('href="https://example.com/api(v2)"'));
  check('inline formatting', /<strong>bold<\/strong>/.test(html) && /<em>em<\/em>/.test(html) && html.includes('<del>gone</del>'));
  check('bare url', html.includes('<a href="https://example.com/x">'));
  check('table alignment and escaped pipe', html.includes('text-align:center') && html.includes('a | b') && html.includes('<code>x|y</code>'));
  check('nested and task lists', /<li>one\s*<ul><li>nested<\/li><\/ul><\/li>/.test(html) && html.includes('class="task"') && html.includes(' checked>'));
  check('ordered start', html.includes('<ol start="3">'));
  check('alert', html.includes('alert-warning') && html.includes('Careful.'));
  check('blockquote', html.includes('<blockquote><p>plain quote</p></blockquote>'));
  check('reference link and footnote', html.includes('href="https://docs.example.com" title="Docs"') && html.includes('id="fn-n"'));
  check('mermaid kept as source', html.includes('class="mermaid-src"') && html.includes('A--&gt;B'));
  check('highlight escapes', html.includes('tk-s">&quot;&lt;b&gt;&quot;') && html.includes('tk-c'));
  const iframe = kit.parse('<iframe src="https://example.com/embed" title="Demo"></iframe>');
  check('iframe remains visible text, never an embedded frame', iframe.includes('&lt;iframe') && !iframe.includes('<iframe'));
  check('raw html escaped', html.includes('&lt;script&gt;') && !html.includes('<script>'));
  check('unsafe url blocked', html.includes('<a href="#">x</a>') && html.includes('<br>'));
  check('setext and details', html.includes('<h2>Setext</h2>') && html.includes('<summary>More</summary>'));

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'render-report-'));
  try {
    fs.mkdirSync(path.join(dir, '.git'));
    fs.mkdirSync(path.join(dir, 'src'));
    fs.writeFileSync(path.join(dir, 'src', 'a.ts'), 'export const a = 1;\n');
    fs.writeFileSync(path.join(dir, 'src', 'guide.md'), '# Guide\n\n## Usage\n\n```md\n## Ignored\n```\n\n## Usage\n'.replace(/\n/g, '\n'));
    fs.writeFileSync(path.join(dir, 'src', 'setext.md'), 'Guide\n=====\n\nUsage\n-----\n');
    check('setext section links resolve', resolveRefs('[guide](../../src/setext.md#usage)', path.join(dir, '.octocode', 'octocode-research'), dir)['../../src/setext.md#usage']?.line === 4);
    const linked = resolveRefs('[guide][g]\n\n[g]: ../../src/guide.md#usage\n\n[again](../../src/guide.md#usage-1)\n[bad](../../src/guide.md#ignored)', path.join(dir, '.octocode', 'octocode-research'), dir);
    check('reference definitions resolve section links', linked['../../src/guide.md#usage']?.line === 3);
    check('duplicate heading anchors resolve and fenced headings do not', linked['../../src/guide.md#usage-1']?.line === 9 && !linked['../../src/guide.md#ignored']);

    const rdir = path.join(dir, '.octocode', 'octocode-research');
    fs.mkdirSync(rdir, { recursive: true });
    const md = path.join(rdir, '2025-01-02-topic.md');
    fs.writeFileSync(md, '---\nstatus: Draft\n---\n# Topic\n\nSee `src/a.ts:3` and [a](../../src/a.ts#L1) but not `src/none.ts`. </script><b>\n');
    fs.writeFileSync(path.join(rdir, '2025-01-01-older.md'), '# Older\n');
    const one = buildReport(md);
    const data = JSON.parse(one.html.match(/id="report-data">([\s\S]*?)<\/script>/)[1]);
    check('workspace from .octocode', one.workspace === dir || fs.realpathSync(one.workspace) === fs.realpathSync(dir));
    check('code refs resolve', data.files[0].refs['src/a.ts:3']?.line === 3 && data.files[0].refs['../../src/a.ts#L1'] && !data.files[0].refs['src/none.ts']);
    check('source path recorded', data.files[0].source.endsWith('2025-01-02-topic.md'));
    check('frontmatter and kind', data.files[0].meta.status === 'Draft' && data.files[0].kind === 'Research');
    check('script-safe embedding', !/<\/script><b>/.test(one.html.split('id="report-data">')[1].split('</script>')[0]));
    check('default out beside source', defaultOut(one.abs, false, one.workspace) === md.replace(/\.md$/, '.html'));
    const lib = buildReport(path.join(dir, '.octocode'));
    check('library collects and orders newest first', lib.files.length === 2 && lib.files[0].endsWith('2025-01-02-topic.md'));
    check('library default out', defaultOut(lib.abs, true, lib.workspace) === path.join(dir, '.octocode', 'index.html'));
    check('outside source goes to the skill view folder', defaultOut(path.join(dir, 'docs', 'x.md'), false, dir) === path.join(dir, '.octocode', KINDS[path.basename(SCRIPT_SKILL_DIR)] ? path.basename(SCRIPT_SKILL_DIR) : 'octocode-skills', 'view', 'x', 'index.html'));
    fs.writeFileSync(path.join(rdir, '2025-01-03-full.md'), '---\nstatus: Confirmed\nconfidence: high\nquestion: Where does it go?\nnext: Ship it\nowner: me\n---\n# Full\n\nThe answer is here,\nacross two lines.\n\n## Evidence\n\nSee `src/a.ts:3` and https://example.com/a.\n\n```md\n# not a heading\n```\n\n### Gaps\n');
    const man = buildManifest(path.join(dir, '.octocode'));
    const full = man.documents[0];
    check('manifest lists every report newest first', man.count === 3 && full.path === '.octocode/octocode-research/2025-01-03-full.md');
    check('manifest status tone', full.statusTone === 'done');
    fs.writeFileSync(path.join(rdir, '2025-01-05-invalid.md'), '---\nstatus: Done\nquestion: Q?\n---\n# Invalid\n\nLead.\n');
    check('invalid status has no tone', buildManifest(path.join(dir, '.octocode')).documents.find((d) => d.path.endsWith('invalid.md')).statusTone === null);
    fs.rmSync(path.join(rdir, '2025-01-05-invalid.md'));
    check('manifest header fields', full.status === 'Confirmed' && full.confidence === 'high' && full.question === 'Where does it go?' && full.next === 'Ship it' && full.meta.owner === 'me');
    check('manifest outline', full.title === 'Full' && full.lead === 'The answer is here, across two lines.' && full.headings.length === 2 && full.headings[1].text === 'Gaps');
    fs.writeFileSync(path.join(rdir, '2025-01-04-table.md'), '# Table first\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nThe lead after the table.\n');
    const tableFirst = buildManifest(path.join(rdir, '2025-01-04-table.md')).documents[0];
    check('manifest lead after a leading table', tableFirst.lead === 'The lead after the table.');
    const today = localDate(new Date());
    const reportDir = path.join(dir, '.octocode', 'octocode-architect');
    fs.mkdirSync(path.join(reportDir, `${today}-folder`), { recursive: true });
    fs.writeFileSync(path.join(reportDir, '2000-01-01-old.md'), '# Old\n\nNo header.\n');
    fs.writeFileSync(path.join(reportDir, `${today}-good.md`), '---\nstatus: Needs work\nquestion: Q?\n---\n# Good\n\nLead.\n\n## Findings\n\n| a |\n|---|\n| 1 |\n\n## Decision\n\nKeep.\n\n## Checks\n\nNone.\n');
    fs.writeFileSync(path.join(reportDir, `${today}-folder`, 'notes.md'), '---\nstatus: Confirmed\nquestion: Q?\n---\n# F\n\nLead.\n');
    const warn = (f) => buildReport(path.join(reportDir, f)).warnings.join('\n');
    const own = contractFor('octocode-architect');
    if (own) {
      check('report check passes a contract-shaped report', warn(`${today}-good.md`) === '');
      const maintained = path.join(reportDir, `${today}-good.md`);
      const edited = new Date(Date.now() + 2 * 24 * 60 * 60 * 1000);
      fs.utimesSync(maintained, edited, edited);
      check('editing a report preserves its original date and links', warn(`${today}-good.md`) === '');
      check('report check flags date and missing header', /written/.test(warn('2000-01-01-old.md')) && /no status/.test(warn('2000-01-01-old.md')) && /no question/.test(warn('2000-01-01-old.md')));
      check('report check flags folder, status word, and section', ((w) => /single report is one file/.test(w) && /"Confirmed" is not one of/.test(w) && /## Findings/.test(w))(warn(`${today}-folder/notes.md`)));
    }
    {
      const run = path.join(reportDir, `${today}-run`);
      fs.mkdirSync(path.join(run, 'cases'), { recursive: true });
      fs.writeFileSync(path.join(run, 'README.md'), '---\nstatus: Confirmed\nquestion: Q?\n---\n# Run\n\nLead.\n');
      fs.writeFileSync(path.join(run, 'cases', 'case-01.md'), '# Case 1\n\nTranscript.\n');
      fs.writeFileSync(path.join(run, 'cases', 'README.md'), '# Cases\n');
      check('supporting files in a report folder are not checked', !/case-01|cases\/README/.test(buildReport(run).warnings.join('\n')));
      check('supporting files stay out of the manifest', !buildManifest(path.join(dir, '.octocode')).documents.some((d) => d.path.includes('case-01')));
      const bare = path.join(reportDir, `${today}-bare`);
      fs.mkdirSync(bare);
      for (const n of ['a.md', 'b.md']) fs.writeFileSync(path.join(bare, n), '# Note\n');
      const bareWarnings = buildReport(bare).warnings;
      check('a folder without an entry point warns once', bareWarnings.length === 1 && /needs README.md/.test(bareWarnings[0]));
      fs.rmSync(run, { recursive: true });
      fs.rmSync(bare, { recursive: true });
    }
    fs.mkdirSync(path.join(dir, '.octocode', 'tmp', 'agents', 'x'), { recursive: true });
    check('report check flags a report left in scratch space', /catalog skips/.test(checkReport(path.join(dir, '.octocode', 'tmp', 'agents', 'x', 'r.md'), { status: 'Ready', question: 'Q?' }, '', dir).join('')));
    fs.mkdirSync(path.join(dir, '.octocode', 'octocode-research', 'tmp'), { recursive: true });
    check('report check flags a report left in a skill tmp folder', /catalog skips/.test(checkReport(path.join(dir, '.octocode', 'octocode-research', 'tmp', 'r.md'), { status: 'Ready', question: 'Q?' }, '', dir).join('')));
    const ap = contractFor('octocode-agentic-prompts');
    if (ap) {
      const apDir = path.join(dir, '.octocode', 'octocode-agentic-prompts');
      fs.mkdirSync(apDir, { recursive: true });
      const mk = (name, sec) => { fs.writeFileSync(path.join(apDir, `${today}-${name}.md`), `---\nstatus: Ready\nquestion: Q?\n---\n# T\n\nLead.\n\n${sec}\n`); return buildReport(path.join(apDir, `${today}-${name}.md`)).warnings.join('\n'); };
      check('either-or sections accept a review or a change report', mk('review', '## Findings') === '' && mk('change', '## Changes') === '');
      check('either-or sections flag a report with neither', /## Findings or ## Changes/.test(mk('neither', '## Notes')));
    }
    const link = path.join(dir, 'linked-skill');
    fs.symlinkSync(path.dirname(path.dirname(entryFile)), link);
    const viaLink = spawnSync(process.execPath, [path.join(link, 'scripts', path.basename(entryFile)), '--help'], { encoding: 'utf8' });
    {
      const sk = path.join(dir, '.octocode', 'octocode-roast');
      fs.mkdirSync(sk, { recursive: true });
      const f2 = path.join(sk, `${localDate(new Date())}-meta.md`);
      fs.writeFileSync(f2, '---\nstatus: Needs work\nquestion: q\nnext: None.\ncaptures: .octocode/tmp/missing/\n---\n# T\n\nLead.\n\n## Findings\n\nx\n');
      const w = buildReport(f2, { workspace: dir }).warnings.join('\n');
      check('flags next none and a missing captures folder', /next` says none/.test(w) && /captures: .octocode\/tmp\/missing\/` does not exist/.test(w));
    }
    check('runs when called through a symlinked skill folder', viaLink.status === 0 && /render-report/.test(viaLink.stdout));
    check('report check skips files outside a skill folder', checkReport(path.join(dir, 'src', 'a.ts'), {}, '', dir).length === 0);
    fs.writeFileSync(path.join(rdir, 'index.html'), '<!doctype html>');
    check('manifest html falls back to the folder page', buildManifest(path.join(rdir, '2025-01-04-table.md')).documents[0].html === '.octocode/octocode-research/index.html');
    fs.rmSync(path.join(rdir, 'index.html'));
    check('manifest links and refs', full.links[0] === 'https://example.com/a' && full.codeRefs[0].path === 'src/a.ts' && full.codeRefs[0].line === 3 && full.skill === 'octocode-research');
    let syntaxOk = true;
    try { new Function(one.html.split('<script>')[1].split('</script>')[0]); } catch { syntaxOk = false; }
    check('page script parses', syntaxOk);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  const failed = cases.filter((c) => !c.ok);
  for (const c of failed) console.error(`FAIL ${c.name}`);
  console.log(failed.length ? `self-test failed (${failed.length}/${cases.length})` : `self-test passed (${cases.length} cases)`);
  return failed.length ? 1 : 0;
}
