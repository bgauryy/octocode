1. **Helped:** The first call, `gh api -X GET repos/fastapi/fastapi/contents/fastapi/security?ref=4b3949cd9e --jq '.[].name'`, listed the modules at the pinned commit. The third call fetched each module with the raw Accept header and grepped for `^class |^from \.`. That one call gave the class names, line numbers, base classes and the `__init__` re-exports for all six files.

2. **Did not help:**
   - **Wasted call:** my second call added `2>/dev/null`. The sandbox blocks `/dev/null`, so every iteration of the loop failed and returned nothing. I had to repeat the call without the redirect.
   - **Noise:** every call printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless once I stopped redirecting, but it made real failures harder to spot.
   - **Gaps:** I never opened `utils.py`. I never read the body of `SecurityBase`. The grep matched only top-level `class` lines, so nested classes would not show.

3. **Next time:** I would skip the redirect and fetch the files in one call from the start. I would also add `utils.py` to the grep, and print the `SecurityBase` body with `sed -n`.

4. **Confidence:** High for the module list, class list and inheritance, because each came straight from the pinned-commit source. Medium for completeness. My statement that `utils.py` holds helper functions was an assumption, and I flagged it as one in the answer.