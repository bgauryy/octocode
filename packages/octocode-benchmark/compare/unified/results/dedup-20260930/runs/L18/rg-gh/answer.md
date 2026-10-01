Rejected elements are left out of the result, and the parser builds the value from what remains. It does this with a SAX-style DOM builder, `json_sax_dom_callback_parser` (`include/nlohmann/detail/input/json_sax.hpp:509`). Its `handle_value` (line 1006) drops rejected values as they are added, so nothing rejected is ever stored. The same file has a few special cases: rejected containers, object members, duplicate keys, and a rejected top-level value.

**Entry point.** `parser.hpp:100` creates `json_sax_dom_callback_parser` when a callback is supplied and runs `sax_parse_internal` with it. Nothing is filtered after the fact.

**Scalars (`handle_value`, `json_sax.hpp:1006-1070`):**
- If the enclosing container was already rejected (`!keep_stack.back()`), the value is skipped without calling the callback (line 1012).
- Otherwise the callback is called with `parse_event_t::value` (line 1025).
- If the callback returns false, the function returns `{false, nullptr}` and nothing is added. For an object member it first removes the placeholder that `key()` stored, via `remove_discarded_value` (lines 1032-1044).
- If the callback accepts the value, it is appended to the parent array (line 1067). If there is no parent, it becomes `root` (line 1050). For an object member it overwrites the placeholder slot (`*object_element = std::move(value)`).

**Object keys (`key()`, lines 620-647):**
- The callback is called with `parse_event_t::key`, and the result is pushed on `key_keep_stack`.
- If the key is kept, `obj[val] = discarded` stores a placeholder and remembers it in `object_element`. The value is written there later, or the placeholder is removed if the value is rejected.
- If the key already exists, its previous value is stashed in `duplicate_key_stash`. If the new value is rejected, the old one is restored instead of being erased (lines 636-644, 978-985).

**Containers:**
- `start_object` and `start_array` call the callback with `object_start` or `array_start` and push the result on `keep_stack` (lines 586-587, 707-708). They then call `handle_value(..., skip_callback=true)` and push the resulting pointer onto `ref_stack`. That pointer is `nullptr` if the container was rejected, so its children are skipped.
- `end_object` and `end_array` call the callback with `object_end` or `array_end` (lines 654, 752). If it returns false, the finished container is overwritten with `discarded` (lines 661, 777). Once the container is popped off the stacks, `remove_discarded_value` (defined at line 964, called at lines 699, 807) removes it from its parent. For an array this pops the last element (line 800). For an object it erases the key, or restores the stashed duplicate value.

**Top level (`parser.hpp:128-133`):** if the whole document was rejected, `result` is still the discarded value, and the parser sets it to `nullptr` (JSON null). If there was a parse error, the result is `discarded` instead (lines 121-125).

**Uncertainty:** I read the code but did not run it, so this is not checked against test output. I also did not trace `resolve_duplicate_key_stash` in detail, so the duplicate-key behavior above is inferred from its call sites and comments.