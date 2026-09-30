1. **Helped:** The single `rg -n "lifo|LIFO" worker.rs` call found every relevant site at once. The follow-up `sed` reads of `:115-127`, `:262-270`, `:700-796` and `:1370-1430` gave the actual logic and comments. The `git log -1` call in the same command confirmed the checkout was at the pinned SHA.

2. **Did not help:** The `rg lifo ../../task/mod.rs` call returned nothing, so it was wasted. I also never read the `builder.rs` docs.

   Some citations in my answer were weaker than I implied:
   - **`:1373-1375` (inject queue fallback):** my `sed` output began mid-function, so I only saw the tail. I inferred the "no core" context rather than reading it.
   - **`:479-481` (park/shutdown):** I saw only the grep comment lines, not the surrounding function. "Parks or shuts down" is an inference.
   - **"Unstable option":** I asserted this without seeing the `cfg` or doc text.

   I disclosed only the `builder.rs` gap.

3. **Next time:** After the grep, I'd read `:470-485` and `:1340-1380` and the `builder.rs` docs before citing them. I'd also skip speculative greps like the `task/mod.rs` one.

4. **Confidence:** High for the core mechanism, the cap of 3, the budget check and the yield case, because I read that code directly. Medium for the park/shutdown, inject-queue and unstable-option details, which are lightly inferred.