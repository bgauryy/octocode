**Short answer:** Newtonsoft.Json enforces required members after it has read the JSON object's properties. It records whether each contract property appeared in the JSON and whether its value was null. Then `EndProcessProperty` throws a `JsonSerializationException` for any property that breaks its `Required` setting. All paths are in `Src/Newtonsoft.Json/Serialization/`.

**1. Where `Required` comes from (`DefaultContractResolver.cs`)**
- `[JsonProperty(Required = ...)]` is copied to `property._required` at line 1548.
- `[JsonRequired]` sets `Required.Always` at line 1587. The attribute is read at line 1519.
- `[DataMember(IsRequired = true)]` gives `Required.AllowNull`, otherwise `Required.Default` (line 1577).
- A class-level `ItemRequired` is the fallback when the property has no setting of its own.

**2. Tracking presence while reading (`JsonSerializerInternalReader.cs`)**
- Line 2467 builds a `Dictionary<JsonProperty, PropertyPresence>` only if `contract.HasRequiredOrDefaultValueProperties` is true or `DefaultValueHandling.Populate` is set. Line 2468 starts every property at `PropertyPresence.None`.
- As each JSON property is read, `SetPropertyPresence` (line 2726, called at lines 2526 and 2543) changes the entry to `Null` or `Value` (lines 2735–2743).
- The constructor-based path does the same with its own `Presence` field (lines 2048–2121, 2141, 2182).

**3. Enforcement (`JsonSerializerInternalReader.cs`)**
- After the object has been read, line 2583 loops over the dictionary and calls `EndProcessProperty` at line 2588. The constructor path calls it at line 2280.
- `EndProcessProperty` is at lines 2671–2710. It only acts when the presence is `None` or `Null`.
- It resolves the setting at line 2677 as `property.Ignored ? Required.Default : property._required ?? contract.ItemRequired ?? Required.Default`.
- If the property is missing (`None`) and the setting is `AllowNull` or `Always`, it throws `"Required property '{0}' not found in JSON."` (lines 2682–2685).
- If the property is present with a null value and the setting is `Always`, it throws `"Required property '{0}' expects a value but got null."` (lines 2701–2704).
- If the property is present with a null value and the setting is `DisallowNull`, it throws `"Property '{0}' expects a non-null value."` (lines 2705–2708).
- When a property is missing and no exception is thrown, the same method applies `DefaultValueHandling.Populate` defaults (lines 2687–2698).
- The `catch` block at line 2712 passes the exception to `IsErrorHandled`. If an error handler deals with it, the error is swallowed. Otherwise the exception is rethrown.

**Writing:** `JsonSerializerInternalWriter.cs:596–603` applies a similar rule when serializing null values. `Always` and `DisallowNull` throw there.

**Not verified:** I didn't read the lines at 2048–2280 in detail. The constructor-path claims come from search hits only.