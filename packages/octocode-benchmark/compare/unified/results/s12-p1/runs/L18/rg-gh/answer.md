**Answer:** When a parser callback is given, the parser uses a different SAX handler, `json_sax_dom_callback_parser`. It builds the DOM as usual, but it asks the callback about each element. It never inserts a rejected element, or it removes the element straight afterwards. The result is the original structure minus the rejected parts. If the top-level value is rejected, the result is `null`. All paths below are under `include/nlohmann/detail/input/`.

**Mechanism** (`json_sax.hpp`, class `json_sax_dom_callback_parser`, starting at line 509):

- **Scalars:** `handle_value` (line 1006) builds the value and calls `callback(depth, parse_event_t::value, value)` (line 1025).
  - If the callback returns false, nothing is stored. `handle_value` returns `{false, nullptr}` (lines 1027–1046).
  - Otherwise the value becomes `root` if `ref_stack` is empty (lines 1048–1052). If not, it is appended to the parent array (lines 1065–1068) or assigned to the object slot (lines 1072–1096).
- **Objects and arrays:**
  - `start_object` and `start_array` call the callback with `object_start` or `array_start` (lines 586 and 707) and push the result onto `keep_stack` (lines 587 and 708).
  - If that result is false, `handle_value` returns `{false, nullptr}` immediately (lines 1012–1015). Every child of that container is therefore skipped.
  - A `nullptr` is pushed onto `ref_stack` for the container (lines 594–595 and 713–714). Children check `!ref_stack.back()` and skip themselves (lines 1056–1059).
  - `end_object` and `end_array` call the callback again with `object_end` or `array_end` (lines 654 and 752). If that call rejects the container, its contents are overwritten with `discarded` (lines 661 and 777).
  - The discarded placeholder is then removed from the parent (lines 696–699 and 795–807). A rejected array element is removed with `pop_back()` (line 800), and a rejected object member is removed by key (line 807).
- **Object keys:**
  - `key()` calls the callback with `parse_event_t::key` (line 625) and records the answer in `key_keep_stack` (line 626).
  - If the key is kept, it inserts a `discarded` placeholder at that key (lines 632–644).
  - If the value is later rejected in `handle_value`, the placeholder is removed (lines 1032–1043).
  - If the key is rejected, no slot is stored. `handle_value` then drops the value because `store_element` is false (lines 1076–1082).
- **Removing placeholders:** `remove_discarded_value` (line 964) erases the trailing `discarded` array element, or the object entry for that key, but only if it is still a `discarded` value.
  - In this checkout it first calls `resolve_duplicate_key_stash(..., true)`. That restores a previously stashed value for a duplicate key instead of erasing it (lines 974–982).
  - I did not read the stash logic itself.

**Top-level handling** (`parser.hpp`):

- If the top-level result is still `discarded` after parsing, it is set to `nullptr` (lines 128–133).
- If parsing errored, the result is `value_t::discarded` instead (lines 121–125). I did not trace what the callback-parse path does with that on the way back to the caller, for example whether it throws.

**Uncertainty:** I read only the code paths above. I did not read the `parse()` wrapper in `json.hpp`, and I did not read the tests.