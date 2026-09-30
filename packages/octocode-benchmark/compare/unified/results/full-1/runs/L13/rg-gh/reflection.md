**Helped:**
- The first `rg -n` call located `getCommand`, `getGenericCommand` and `expireIfNeeded` in one step. It also confirmed HEAD was 20bb2cfc54 via `git rev-parse`.
- The second call was a `sed -n` over `src/db.c:2940-3110`. It showed `keyIsExpired`, the `expireIfNeeded` doc comment and its full body, which answered most of the question.
- The third call's `sed` on `src/db.c:300-362` showed how `lookupKey` handles `KEY_VALID` and the miss path. Its `rg -A` showed `deleteKeyAndPropagate`.

**Did not help:**
- `sed` output has no line numbers, so I worked out most line citations by counting from the start of each range. Examples are `db.c:3073-3082`, `3050-3053`, `3058-3059`, `3070` and `3010-3019`. They could be off by a few lines. I should have used `rg -n` or `nl`, and I didn't say this in the answer.
- I never read `lookupKeyRead` itself. "No extra flags for a plain GET" is inferred from the flag mapping, though I did note that in the answer.
- I also didn't read `propagateDeletion` or `dbGenericDelete`.

**Next time:** I'd use `rg -n` with context, or `nl -ba | sed -n`, so every cited line is printed. I'd also read `lookupKeyRead` directly.

**Confidence:** High on the behavior, because I read the control flow directly. Medium on the exact line numbers for the derived ranges.