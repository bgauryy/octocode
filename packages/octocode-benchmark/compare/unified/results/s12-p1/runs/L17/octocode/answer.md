Required members are enforced by tracking which properties appeared in the JSON while the object is read. After the last property, each tracked property is checked against its `Required` setting. This is in `Src/Newtonsoft.Json/Serialization/JsonSerializerInternalReader.cs`. I did not read how `HasRequiredOrDefaultValueProperties` is computed, or the metadata that sets `JsonProperty._required`.

**1. Presence tracking (`PropertyPresence`: None, Null, Value)**
- Tracking is only switched on when needed. The comment at line 2466 says "only need to keep a track of properties' presence if they are required or a value should be defaulted if missing".
- Lines 2467-2469 create a `Dictionary<JsonProperty, PropertyPresence>` with every property set to `None`. That happens only if `contract.HasRequiredOrDefaultValueProperties` is true or `DefaultValueHandling.Populate` is set. Otherwise the dictionary is null.
- As each property is read, `SetPropertyPresence` is called (lines 2526 and 2543; definition at 2726-2749).
  - A JSON `null` or `undefined` token records `Null`.
  - A string that `CoerceEmptyStringToNull` treats as null also records `Null`.
  - Anything else records `Value`.

**2. Check after the last property**
- Lines 2581-2589 loop over the dictionary and call `EndProcessProperty(...)` for each property.
- `EndProcessProperty` (lines 2671-2724) works out the effective setting at line 2677: `property.Ignored ? Required.Default : property._required ?? contract.ItemRequired ?? Required.Default`. A per-property setting therefore overrides the contract-level `ItemRequired`, and ignored properties are never required.
- The checks apply only when presence is `None` or `Null`:
  - Presence `None` with `Required.AllowNull` or `Required.Always` throws "Required property '{0}' not found in JSON." (lines 2682-2685).
  - Presence `Null` with `Required.Always` throws "Required property '{0}' expects a value but got null." (lines 2701-2704).
  - Presence `Null` with `Required.DisallowNull` throws "Property '{0}' expects a non-null value." (lines 2705-2708).
  - A property that is missing but not required may get its default value populated, depending on `DefaultValueHandling` (lines 2687-2698).
- The exception goes through `IsErrorHandled(...)` (line 2714). If an error handler marks it handled, `HandleError` runs and deserialization continues. Otherwise the exception is rethrown.

**3. Constructor-based creation**
- For objects built through a parameterized constructor, presence is tracked per `CreatorPropertyContext` (lines 2048, 2088-2092, 2105-2124, 2141). `EndProcessProperty` is also called from line 2280, which I saw only as a search hit and did not read.

**Related and not the same:** `MissingMemberHandling.Error` (lines 2382 and 2505) rejects JSON members that the type does not have. It does not enforce required members.