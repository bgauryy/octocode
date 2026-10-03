1. **Helped:** The first successful `ghGetFileContent` call (`matchString: "def resolve_redirects"`, `contextLines: 120`) returned most of the loop, `get_redirect_target` and `should_strip_auth` in one read. The `startLine: 300–450` read then gave `rebuild_auth`, `rebuild_proxies` and `rebuild_method`. The `matchString: "allow_redirects"` query showed how `send` drives the generator.

2. **Did not help:**
   - My first call failed validation because I omitted the `queries[]` wrapper. I should have checked the schema first.
   - The 8000-character chunk cut off mid-statement at `rewindable`.
   - The `startLine: 268–302` read mostly duplicated what I already had. It did fill the cut-off section.
   - The `allow_redirects` query returned "lines omitted" markers, so I never saw the middle of `send`. The hooks handling and `history` bookkeeping there are unread.
   - Results carried `confidence: medium`, `partial: true` and `contentView: none`. The returned content had no line numbers, so I could only cite `def resolve_redirects` at about line 186, from `matchedLines`.
   - I didn't pin or verify that `611c6162cb` resolved as the commit. The tool just accepted it as a branch.

3. **Next time:** Use the `queries[]` wrapper from the start. Read an explicit line range such as 180–340 in one call to get the whole loop. Then read the `send` range separately. That would yield line numbers I could cite.

4. **Confidence:** Medium-high on the behavior, because I read the actual code for every claim about the loop and the rebuild methods. Medium on citations, because I gave function names instead of line numbers, and I haven't read the middle of `send`.