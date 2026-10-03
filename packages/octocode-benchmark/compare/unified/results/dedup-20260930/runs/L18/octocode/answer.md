**Short answer:** When you pass a callback, the parser builds the DOM with `json_sax_dom_callback_parser` (`include/nlohmann/detail/input/json_sax.hpp`). Each SAX event calls the callback. A rejected value is never added to its parent, or it is added as a `discarded` placeholder and then removed. So the result is the input tree with the rejected elements pruned out. If the root itself is rejected, the result is `null`.

**Mechanics** (line numbers are in `json_sax.hpp` unless noted):

- **Scalars** (`null`, `boolean`, numbers, `string`, `binary`) all go through `handle_value` (around lines 1008–1100).
  - If `keep_stack.back()` is false, the enclosing container was rejected, so nothing is built and it returns `{false, nullptr}` (line 1012).
  - Otherwise it builds the value and calls `callback(depth, parse_event_t::value, value)`.
  - If the callback returns false, the value is not stored. For an object member, it also removes the placeholder that `key()` stored (lines 1034–1043).
  - If it is kept, it becomes the root when `ref_stack` is empty, is `emplace_back`ed into an array, or is assigned to `*object_element` for an object.
- **Objects and arrays:** `start_object` and `start_array` (around lines 580–600) call the callback with `object_start` or `array_start`.
  - The result is pushed on `keep_stack`.
  - `handle_value(..., skip_callback=true)` then inserts an empty container, or returns `nullptr` if it was rejected.
  - The pointer goes on `ref_stack`, so children of a rejected container are skipped.
  - At `end_object` and `end_array` the callback is called again with `object_end` or `array_end`. If it returns false, `*ref_stack.back() = discarded` (lines 689–695). The parent then calls `remove_discarded_value`, which drops the placeholder (line 699 for objects, line 807 for arrays).
- **Keys:** `key()` calls the callback with `parse_event_t::key`.
  - The result goes on `key_keep_stack`.
  - If the key is kept, `obj[val] = discarded` inserts a placeholder and sets `object_element`. The value handler later overwrites that placeholder or removes it.
  - If the key is rejected, nothing is inserted, and the value is dropped because `store_element` is false.
- **`remove_discarded_value`** (line 964) looks only at the last array element, or at `object.find(key)`. It pops or erases that element if `is_discarded()`.
- **Duplicate keys:** this version stashes the previous value in `duplicate_key_stash`. If the new value is rejected, the old one is restored instead of erased.
- **Root rejected:** in `parser.hpp:130`, `if (result.is_discarded()) result = nullptr;`, so a rejected top-level value yields `null`.

**Uncertainty:** I did not read the array-side handlers (`end_array`, around lines 720–810) in full, only the object side and the `remove_discarded_value` calls at line 807. I also did not read the `duplicate_key_stash` helper, `resolve_duplicate_key_stash`, in detail.