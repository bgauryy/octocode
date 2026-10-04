1. **Helped:** The second Bash call, `grep -n "func extrapolatedRate|funcRate|funcIncrease|funcDelta" promql/functions.go`, located the entry points at lines 452, 806, 811 and 816. The third call, a `sed -n 440,640p` plus `sed -n 800,822p` of `promql/functions.go`, gave the whole algorithm with comments in one read, so no further searching was needed.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so I had to fall back to `grep`.
   - Every Bash call printed `/dev/null: Operation not permitted`. This was noise, and it made `git rev-parse HEAD` fail, so I never confirmed the checkout was at ea954809ce.
   - `sed` output has no line numbers, so I could cite only the four lines from `grep -n`. The line numbers for the internal steps (reset handling, extrapolation, scaling) are missing from my answer.
   - I never opened `histogramRate`, `isStartTimestampReset`, `extendedRate` or `extendedHistogramRate`, and the answer says so.

3. **Next time:** I'd use `grep -n` or `nl -ba` on the same ranges to get line numbers for every claim. I'd also try the commit check another way, such as reading `.git/HEAD` directly. I'd read `extendedRate` too, since smoothed and anchored selectors are a real branch.

4. **Confidence:** Medium-high on the algorithm, because I read the code directly. Medium on the commit pin, because I couldn't verify it. Low on the helper functions I didn't read.