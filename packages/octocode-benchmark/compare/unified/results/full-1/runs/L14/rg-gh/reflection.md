1. **Helped:** The first call did most of the work. `git rev-parse HEAD` confirmed the pinned commit, and `rg -n "function debounce"` gave the location (`lodash.js:10403`) in one step. The second call printed lines 10403-10525 in full, so I could read the whole implementation at once. The `cat -n | awk` pipeline added line numbers, which let me cite exact lines.

2. **Did not help:** My first `ls debounce.js` failed with "No such file", because this checkout is the monolithic `lodash.js`. That cost a little. The awk numbering left the source without indentation and made it harder to read. I ran no tests, so I never saw the behaviour.

3. **Next time:** I'd skip guessing at a per-function file and go straight to `rg`. I'd use `rg -n` or `sed -n` with `nl -ba`, which keeps indentation. I might also check `test/` for debounce and `maxWait` tests to confirm the behaviour.

4. **Confidence:** High for the mechanics, because I read the source directly at the confirmed commit and cited its line numbers. Medium-high for the summary sentence on `maxWait` guarantees, since that is my inference from the code, not something I ran.