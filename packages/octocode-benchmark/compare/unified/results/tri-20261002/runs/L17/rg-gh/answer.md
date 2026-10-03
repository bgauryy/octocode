**Short answer:** Newtonsoft.Json enforces required members after it has read the whole JSON object. While reading, it records whether each property was missing, null or had a value. Once the object ends, it checks that record against each property's resolved `Required` setting and throws a `JsonSerializationException` on a violation. All line numbers below are in `Src/Newtonsoft.Json/Serialization/`.

**1. Tracking is switched on only when needed.**
- `JsonObjectContract.HasRequiredOrDefaultValueProperties` (`JsonObjectContract.cs:143-167`) returns true if `ItemRequired` is not `Default`. It also returns true if any property has `Required != Default` or `DefaultValueHandling.Populate`.
- The reader uses that flag (or a global `Populate`) to turn on presence tracking. In `JsonSerializerInternalReader.cs`, the populate path does this at line 2467 and the creator (parameterized constructor) path at line 2065.

**2. Presence is recorded while reading properties.**
- `propertiesPresence` starts with every contract property set to `PropertyPresence.None` (`JsonSerializerInternalReader.cs:2468`).
- `SetPropertyPresence` (around line 2724-2745) then marks each property it reads as `Null` or `Value`.
- The constructor path does the same through `CreatorPropertyContext.Presence` (lines 2091-2121).

**3. The check runs once the object has been read.**
- After the end of the object is confirmed, the populate path loops over `propertiesPresence` and calls `EndProcessProperty` for each entry (lines 2583-2589). This happens before `OnDeserialized`.
- The constructor path does the same at lines 2264-2290.

**4. `EndProcessProperty` (lines 2671-2718) applies the rules.** The resolved setting is `property.Ignored ? Default : property._required ?? contract.ItemRequired ?? Default` (line 2677). So a per-property `Required` overrides the contract's `ItemRequired`, and ignored properties are never required.
- **Property absent (`None`):** with `Required.Always` or `AllowNull`, it throws `"Required property '{0}' not found in JSON."` (lines 2682-2685).
- **Property is JSON null (`Null`):**
  - `Always` throws `"Required property '{0}' expects a value but got null."` (lines 2701-2704).
  - `DisallowNull` throws `"Property '{0}' expects a non-null value."` (lines 2705-2708).
- **Error handling:** the check is wrapped in a try/catch that goes through `IsErrorHandled`. If an `Error` event handler handles the exception, deserialization continues. Otherwise the exception is rethrown (lines 2712-2720).

**Related setting.** `MissingMemberHandling.Error` is a different check. It throws for JSON members that have no matching property on the type, not for missing required ones (lines 2382 and 2505).

**Not checked.** I did not read the `Required` enum docs, the `[JsonProperty(Required=…)]` attribute plumbing, or the C# `required` keyword handling. I only traced the enforcement path in the reader.