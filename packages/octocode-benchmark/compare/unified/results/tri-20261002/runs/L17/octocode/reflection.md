**1. Helped:**
- The first `localSearch`, a regex for `Required.Always|DisallowNull|...` scoped to `Serialization/`, hit `EndProcessProperty` and the resolver in one call.
- `localFetch` on lines 2655-2720 showed the whole enforcement switch, including the resolution order and the error-handler `catch`.
- The second `localSearch` (`PropertyPresence|EndProcessProperty|...`) mapped how presence is tracked and where `_required` is set.

**2. Did not help:**
- The first search's pattern included `MissingRequiredMember` and `IsRequiredMember`, which have no matches.
- I never fetched the code around lines 2467-2590 or 2040-2290. Those claims rest on one-line search snippets: that the loop runs after the object is read, the constructor path, and the `SetPropertyPresence` call sites.
- I didn't open `HasRequiredOrDefaultValueProperties`, `JsonProperty.cs:206` or `DefaultContractResolver.cs:277`.
- The writer behavior came from a snippet plus its error text. I inferred the "null value" context rather than reading it.
- I only flagged the constructor-path gap in the answer. The "after the object has been read" ordering was also inferred, and I didn't say so.

**3. Next time:** I'd make two `localFetch` calls on 2460-2600 and 2040-2290 to confirm the ordering and the constructor path. I'd also read the `HasRequiredOrDefaultValueProperties` definition.

**4. Confidence:** High for the core mechanism, because I read lines 2671-2710 and the resolver lines directly. Medium for the claims about reading order and the constructor path, which I did not verify.