Required members are enforced by tracking which properties appear in the JSON. After the closing `}` is read, any property that was missing, or present but null, is checked against its `Required` setting. All paths below are in `Src/Newtonsoft.Json/Serialization/`.

**1. Tracking is switched on only when needed**
- `JsonObjectContract.HasRequiredOrDefaultValueProperties` (`JsonObjectContract.cs:143-170`) is true if `ItemRequired` is not `Default`, or if any property has `Required != Default` or `DefaultValueHandling.Populate`.
- In the reader's populate path, `JsonSerializerInternalReader.cs:2467-2469` builds `propertiesPresence`. It is a dictionary with every contract property set to `PropertyPresence.None`. It is built only if that flag is true or the serializer's `_defaultValueHandling` has `Populate`; otherwise it is `null` and nothing is tracked.

**2. Presence is recorded as properties are read**
- `SetPropertyPresence` (`:2726-2747`) writes `propertiesPresence[property]` as `Value` or `Null`. Null JSON tokens and undefined count as `Null`, and so does a string that `CoerceEmptyStringToNull` treats as null (`:2733-2740`).
- It is called both for properties that are deserialized (`:2543`) and for ignored or `ShouldDeserialize`-false properties (`:2526`).

**3. The check runs after the object is fully read**
- After the loop finishes (`:2576-2579`), `:2581-2590` calls `EndProcessProperty(..., presence, true)` for every property in the dictionary.
- The constructor-based path has its own equivalent. `trackPresence` is set at `:2065`, and `EndProcessProperty` is called at `:2280`. I did not read that code.

**4. `EndProcessProperty` decides (`:2671-2724`)**
- It does nothing unless the presence is `None` or `Null` (`:2673`).
- The effective requirement is `property.Ignored ? Default : property._required ?? contract.ItemRequired ?? Default` (`:2677`). A per-property setting overrides the contract's `ItemRequired`.
- **Property absent (`None`):** if the requirement is `AllowNull` or `Always`, it throws `JsonSerializationException` with "Required property '{0}' not found in JSON." (`:2682-2685`). Otherwise it may apply the default value when `DefaultValueHandling.Populate` is set and the property is writable (`:2687-2698`).
- **Property null (`Null`):**
  - `Always` throws "Required property '{0}' expects a value but got null." (`:2701-2704`).
  - `DisallowNull` throws "Property '{0}' expects a non-null value." (`:2705-2708`).
- The check is wrapped in try/catch (`:2712-2722`). If `IsErrorHandled` returns true (an error handler such as `[OnError]` or the serializer's `Error` event handles it), `HandleError` is called and deserialization continues. Otherwise the exception is rethrown.

**Not checked**
- Properties marked `JsonIgnore` have `Ignored` set, so their requirement resolves to `Default` and is never enforced.
- I did not look at how the `Required` attribute values map onto `JsonProperty._required` in the contract resolver. The only resolver reference I saw is `ItemRequired` being set at `DefaultContractResolver.cs:364`.