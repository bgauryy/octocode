**Short answer:** the parser feeds SAX events into `json_sax_dom_callback_parser`, which builds the DOM incrementally. For each event it calls the callback and only keeps what the callback accepts. Rejected values are never stored, or are removed again. If the top-level value is rejected, `parse()` returns `null`. Line numbers are in `include/nlohmann/detail/input/`.

**Mechanism (`json_sax.hpp`)**
- **Setup:** the constructor (`json_sax.hpp:527`) pushes `true` onto `keep_stack`. `ref_stack` holds pointers to the open containers, and a `nullptr` entry marks a container that was rejected.
- **Scalars:** every scalar event goes through `handle_value` (`~990-1090`):
  - If `keep_stack.back()` is false, the parent container was rejected, so it returns `{false, nullptr}` and nothing is built (`1012`).
  - Otherwise it builds the value and calls `callback(depth, parse_event_t::value, value)`. If the callback returns false, it returns `{false, nullptr}` and the value is dropped.
  - If `ref_stack` is empty, the value becomes `root`. In an array it is appended with `emplace_back`. In an object it is written into `*object_element`.
- **Containers:** `start_object` and `start_array` (`586-`) call the callback with `object_start` or `array_start`. The result is pushed onto `keep_stack`. `handle_value(..., skip_callback=true)` then adds the container, and the returned pointer (`nullptr` if rejected) goes onto `ref_stack`. If the container was rejected, everything inside it is skipped through the `keep_stack` check above.
- **Object keys:** `key()` (`~620-650`) calls the callback with `parse_event_t::key`. The result goes onto `key_keep_stack`. If the key is kept, it inserts a `discarded` placeholder at that key and points `object_element` at it.
- **Rejected object member value:** `handle_value` erases the placeholder with `remove_discarded_value` (`1034-1041`). If the key was a duplicate, `duplicate_key_stash` restores the earlier value instead of erasing it.
- **`end_object`** (`~670-720`): it calls the callback with `object_end`. If the callback rejects the object, the slot is overwritten with `discarded` (or the duplicate-key stash restores the earlier value). The stacks are then popped, and `remove_discarded_value(parent, object_key)` erases the discarded slot from the parent object.
- **`end_array`:** I did not read it. By symmetry with `end_object` it should behave the same way, but that is not verified.

**Final result (`parser.hpp`)**
- After parsing, if the result is discarded, i.e. the top-level value was rejected, it is set to `null` (comment at `parser.hpp:128`, check at `:130`).
- On a syntax error, the result is `value_t::discarded` (`:121-124`).

**Uncertainty:** I did not read `remove_discarded_value` or `end_array` directly. The array-side description and the exact line ranges for `key()` and `end_object` are approximate.