1. **Helped:** The first `localSearch` for `Required.Always|...` in `JsonSerializerInternalReader.cs` found `EndProcessProperty` straight away. The second `localSearch` on `Required|requiredProperties|RequiredProp` mapped the whole flow, including the `SetPropertyPresence` definition and the `trackPresence` lines. The `localFetch` of lines 2660-2750 gave the exact rules and messages. The `localFetch` with `matchString: propertiesPresence` and `contextLines: 3` showed how the presence dictionary is initialised and looped over at the end. It omitted the middle lines, but that was fine.

2. **Did not help:**
   - One `localSearch` failed with `invalidRegex`, because I used `(` in a regex search. It cost a retry and should have been `regex: literal` from the start.
   - The `EndProcessProperty(` search returned very long multi-line snippets.
   - I never read the block around line 2280, which is the constructor-based creation path. My answer says so, but it leaves a gap.
   - `HasRequiredOrDefaultValueProperties` has no `lspSearch` references check. I relied on text search.

3. **Next time:** Use `regex: literal` for anything containing punctuation. Read around line 2280 to confirm the constructor path. Check `JsonProperty.Required` and the `[JsonProperty(Required=...)]` attribute handling, and the `JsonRequired` attribute, to round out the picture.

4. **Confidence:** High for the main flow, because I read the source lines. Medium on the constructor-creation path, which I read only through a search snippet.