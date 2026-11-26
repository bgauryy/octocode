Load when reading a wiki page's layout, judging stale or orphaned notes, or fixing a wiki lint issue.

# Wiki pages, freshness and lint

## Page layout

1. `# <community label>` (the deepest shared folder, with the top file when two communities share a folder).
2. Header line: `**Notes:** none yet | fresh (written <date>) | stale: N members changed since the notes were written <date>: <files>`, then `**orphaned**` when no community matches, the community id, the size and the slug.
3. `## Facts`, regenerated on every build; never edit it. It holds size, inside and across weight, chunks and purity, the file-check counts, and these sections:
   - Members by PageRank: every member with `path:1`, language, PageRank, betweenness, in and out weight, depth, k-core, cycle id, check status and line count. No member is left out.
   - Links out and Links in: other pages as `[[slug]]` with summed production weight, file-edge count and the strongest edge as `path:line → path:1`. Test-only edges are excluded, following the weight rule.
   - Cycles: each cycle with members here, its size, weight, languages and chunks.
   - File-check problems: members whose `check` is not `ok`.
   - Most-called symbols: production call sites into symbols declared here, with declaration `path:line`, sites from other pages and caller counts. The section identifies additional rows available in `data.json` `calls`.
4. `## Agent notes`: the text between `<!-- notes:start -->` and `<!-- notes:end -->`, kept verbatim across builds.

## Freshness and identity

- **Stale notes.** When notes are written, map.json records a content hash of every member. A later build or lint compares the hashes with the files on disk. A member that changed, appeared or disappeared makes the notes stale, and the page header and `index.md` name the files. A refresh also passes the files it found changed. Read the changed files before relying on stale notes. Writing fresh notes resets the baseline.
- **Stable slugs.** Communities can be renumbered after a build. The script matches them to previous pages by member overlap, preserving slugs and notes when a match qualifies; otherwise it creates a new page.
- **Orphans.** A previous page that no community takes stays as **orphaned** when it has notes, listing its last members. Without notes it is removed, and the log records it. Move what still holds into the pages that own those files now, then clear the orphan's notes with `note <out> <slug> ""`. The next build removes it.

## Lint issue kinds

| kind | Meaning | Fix |
|---|---|---|
| `citation-file` | A notes citation names no file under the root | Correct the path or drop the claim |
| `citation-line` | The cited line is past the end of the file | Re-read the source and cite the current line |
| `broken-link` | `[[slug]]` names no page | Use a slug from `index.md` |
| `missing-graph` | data.json or its community metrics are missing | Use a completed graph export |
| `missing-page` | A community in `data.json` has no page, or its page predates the current graph | `octocode-graph wiki build <out>` |
| `missing-file` | A wiki page, index or map is missing | `octocode-graph wiki build <out>` |
| `orphaned` | No community matches the page | Move the notes, then clear them ([orphans](#freshness-and-identity)) |
| `stale-notes` | A member changed since the notes were written | Read the changed members, then write the notes again (`wiki note`) |
| `empty-notes` | Only with `--top N`, one of the N highest-PageRank live communities has no notes | Add notes if curating that community |
