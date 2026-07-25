# ghSearchRepos

Discover GitHub repositories by name, topic, language, or popularity. `keywords`
are ANDed; `topicsToSearch` is sparse. keywords + topics merges two searches
(OR). `concise:true` to triage owner/repo strings, then inspect a candidate.

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchRepos --scheme`)

| param | type | notes |
|---|---|---|
| keywords | array<string> | one term per item = broad; a phrase in one item = exact |
| topicsToSearch | array<string> | all topics required; pair with keywords/language |
| language | string | repo language qualifier |
| owner | string | without keywords, enumerates that owner's repos |
| stars / forks | string | range syntax `>100`, `<1000`, `50..500`, `>=500` |
| size | string | KB: `<1000` small, `>10000` large |
| created / updated | string | date range; `updated` = GitHub `pushed:` |
| match | array<enum(name,description,readme)> | which text fields; readme = broader/slower |
| sort | enum(stars,forks,help-wanted-issues,updated,best-match) | ordering |
| limit | int 1–100 | per page |
| page | int 1–1000 | |
| archived | boolean | include archived |
| visibility | enum(public,private) | private needs repo-scoped token |
| license | string | lowercase SPDX id (`mit`, `apache-2.0`) |
| goodFirstIssues | string | count filter `>5` |
| concise | boolean | flat `owner/repo` triage |

## Checks

1. **Keyword + language + stars** — `$CLI tools ghSearchRepos --queries '{"keywords":["react"],"language":"TypeScript","stars":">1000","concise":true,"limit":5}' --compact`
   → PASS: 5 popular TS repos, flat rows.
2. **Owner enumeration** — `... '{"owner":"bgauryy","concise":true,"limit":10}'` → PASS: bgauryy's repos incl. `octocode`.
3. **Topic + keyword merge** — `... '{"keywords":["state"],"topicsToSearch":["react"],"limit":5}'` → PASS: results honor OR-merge semantics.
4. **sort=stars vs best-match** — run with `"sort":"stars"` then `"sort":"best-match"` → PASS: distinct ordering.
5. **Rich mode** — `concise:false` → PASS: stars/language/topics/dates present.
6. **Honest empty** — impossible topic combo → PASS: empty with "drop a topic/filter" hint.

## Workflows

- **Discover → inspect**: `concise:true` triage → pick a candidate → `ghViewRepoStructure` or `ghSearchCode` on it.
- **Contributor scouting**: `goodFirstIssues:">5"` + `language` → shortlist repos inviting contributors.
