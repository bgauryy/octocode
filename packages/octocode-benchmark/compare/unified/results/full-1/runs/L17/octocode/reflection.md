**1. Helped:** The first successful `localSearch` on `JsonSerializerInternalReader.cs` was the fastest step. One regex on `Required\.|HasRequired...` returned the tracking condition (lines 2065 and 2467) and the throw sites (2677–2705). The `localFetch` on lines 2660–2725 then gave the full `EndProcessProperty` logic. The `matchString` query for `EndProcessProperty(` showed the call sites. Its output elided most lines, but the sites at 2280 and 2588 were visible.

**2. Did not help:**
- Both my first `localSearch` and my first `localFetch` failed schema validation. I had passed `contextLines` as a string, and I had left out the required `goal` and `reasoning` fields. That wasted two calls.
- The `matchString` fetch printed "lines omitted" gaps, so I could not see the code around the call sites.
- One `localSearch` value was truncated at 200 characters.

**3. Next time:**
- Include `goal` and `reasoning` on every call, and pass numbers as numbers.
- Read the `PropertyPresence` assignment near line 2467 with a targeted `localFetch`.
- Check how `HasRequiredOrDefaultValueProperties` is computed in `JsonObjectContract`. I skipped both, so the answer has gaps there.

**4. Confidence:** High for the enforcement logic and the error messages, because I read those lines directly. Medium for the overall picture, since I did not read where presence is recorded or where `HasRequiredOrDefaultValueProperties` is set. The answer says so.