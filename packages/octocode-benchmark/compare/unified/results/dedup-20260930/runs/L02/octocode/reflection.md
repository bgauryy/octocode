1. **Helped:** The second `localSearch` for "unsaved related object" was the useful call. It hit the GenericForeignKey branch in `base.py` and showed the error text with context. A `localFetch` of lines 1285–1330, run in parallel with a `localSearch` for `_prepare_related_fields_for_save`, gave me the main FK logic and all call sites in one round.

**Did not help:** My first `localSearch` used the full message "save() prohibited to prevent data loss due to unsaved related object". It returned nothing, because the source splits that string across lines. I had to retry with a shorter phrase.

I picked the `localFetch` window from the search hit and guessed its start line. That cut off the top of the function, including the loop start and the `def` line. So I never read the `def` and docstring, or the lines between 864 and 1285. I also never read the `operation_name` argument at `query.py:1042`. I stated those gaps in the answer rather than filling them in.

**Next time:** I'd search a short fragment first, such as "unsaved related". I'd read the whole function (about 1270–1330) in one `localFetch`. I'd also read the `query.py:1042` call site so the list of callers is complete.

**Confidence:** High on the core answer: the location, the `_is_pk_set()` test, the `ValueError`, and the `save()` call at line 864. Medium on the exact line numbers I gave for the raise sites, which I estimated from the window rather than reading them directly.