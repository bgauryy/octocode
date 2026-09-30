Newtonsoft.Json enforces required members after the JSON object has been read, by checking which properties were actually present. All line numbers below are in `Src/Newtonsoft.Json/Serialization/JsonSerializerInternalReader.cs`. I did not read where presence is recorded, so that step is inferred from the code I saw.

**Presence tracking**
- Tracking is switched on only when `contract.HasRequiredOrDefaultValueProperties` is true or `DefaultValueHandling.Populate` is set (lines 2065 and 2467). This avoids the cost for types that have no required properties.
- Each property gets a `PropertyPresence` value: `None` (absent from the JSON), `Null` (present with a null value), or a value for present and non-null. The tracking dictionary is `propertiesPresence` (line 2467).

**Enforcement in `EndProcessProperty`** (defined around line 2670, checks at 2677–2710)
- After the object is read, each property in the presence dictionary is passed to `EndProcessProperty(newObject, reader, contract, initialDepth, property, presence, true)` (line 2588 call site). A separate call site is at line 2280, for the path that constructs the object late (`createdObject`).
- Only `None` and `Null` presence is examined.
- The effective requirement is `property.Ignored ? Required.Default : property._required ?? contract.ItemRequired ?? Required.Default` (line 2677). An ignored property is never required, and a property-level setting overrides the contract-level `ItemRequired`.
- If the property is absent (`PropertyPresence.None`) and the requirement is `AllowNull` or `Always`, it throws `JsonSerializationException` with the message "Required property '{0}' not found in JSON." (lines 2682–2684).
- If the property is present but null (`PropertyPresence.Null`):
  - `Always` throws "Required property '{0}' expects a value but got null." (lines 2701–2703).
  - `DisallowNull` throws "Property '{0}' expects a non-null value." (line 2705 onward).
  - `AllowNull` is accepted for a null value.
- If the property is absent and the requirement did not throw, a default is set only when `setDefaultValue` is true, the property is not ignored, `DefaultValueHandling.Populate` applies, and the property is writable.

**Error handling**
- The checks sit inside a `try`/`catch`. The catch calls `IsErrorHandled(...)`, so an `OnError` handler or the serializer's `Error` event can swallow the exception (followed by `HandleError`). Otherwise the exception is rethrown.

**Uncertainty**
- I did not read the code that populates `PropertyPresence` while properties are being read (around line 2467).
- I did not read how `HasRequiredOrDefaultValueProperties` is computed on the contract.