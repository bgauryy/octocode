**Short answer:** the result is built by a SAX handler, `json_sax_dom_callback_parser`. It builds the DOM as events arrive and calls the callback at each step. Rejected values are never kept, or they are stored as a `discarded` placeholder and removed later. If the root itself is rejected, the result is `null`. All paths below are under `include/nlohmann/detail/input/`.

**Flow**
- `parser::parse` (`parser.hpp:96-`) creates a `json_sax_dom_callback_parser` bound to the result and runs `sax_parse_internal` on it (`parser.hpp:100-101`).
- The handler keeps `ref_stack`, `keep_stack` and `key_keep_stack` (`json_sax.hpp:1103-1107`). It also keeps `key_stack` and `container_key_stack`, so a rejected value can be removed without searching the parent.
- `ref_stack` holds pointers to the containers under construction. A `nullptr` entry means the container was rejected, so its children are skipped.

**Scalars and containers (`handle_value`, `json_sax.hpp:1006-1097`)**
- If the enclosing container was already rejected (`!keep_stack.back()`), it returns `{false, nullptr}` and the callback is not called (1012-1015).
- Otherwise it builds the value and calls the callback with `parse_event_t::value` (1025). Containers pass `skip_callback=true` here, so the callback doesn't see an empty container.
- If the callback rejects the value (1028-1045), nothing is added. For an object member, the placeholder that `key()` stored is removed via `remove_discarded_value`.
- If the value is kept, it becomes the root when `ref_stack` is empty (1048-1052). Otherwise it is appended to the parent array (1065-1068), or assigned to the object slot that `key()` prepared (1085-1086).

**Object keys (`key`, `json_sax.hpp:620-647`)**
- The callback is called with `parse_event_t::key` (625).
- If the key is kept, a `discarded` placeholder is inserted at that key (644). The later value overwrites it. If the value is rejected, the placeholder is erased.
- If the key is rejected, no placeholder is stored, and the value is then dropped in `handle_value` (1076-1083).
- For a duplicate key, the previous value is stashed (`duplicate_key_stash`, 634-644). It is restored if the new value is rejected.

**Containers**
- `start_object` and `start_array` call the callback with `object_start` or `array_start` and push the result onto `keep_stack` (586-587, 707-708).
- `end_object` (648-) and `end_array` (~748-) call the callback with `object_end` or `array_end`. If it returns false, the container is overwritten with `discarded` (661, 777), unless a stashed duplicate value is restored.
- After popping, `remove_discarded_value` (`json_sax.hpp:963-`) removes the rejected child from its parent. It pops the last array element, or erases the object key, but only if that slot `is_discarded()`.

**Root**
- If the root value itself was rejected, `result` is left as `discarded`. `parser.hpp:130-133` then converts it to `null`.
- On a parse error, `result` is set to `value_t::discarded` instead (`parser.hpp:122-125`). With exceptions enabled, the error is thrown rather than returned.

**Uncertainty**
- I read the `end_array` line numbers (~748-807) from a grep listing and did not view that function in full. The logic mirrors `end_object`.