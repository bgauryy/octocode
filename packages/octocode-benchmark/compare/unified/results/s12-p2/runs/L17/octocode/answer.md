Newtonsoft.Json enforces required members in `JsonSerializerInternalReader.cs` (under `Src/Newtonsoft.Json/Serialization/`). It does this by tracking which properties were seen while reading the object, then checking each one after the closing brace. I haven't traced how `SetPropertyPresence` assigns the presence values; I only read its signature at line 2726.

**1. Presence tracking is optional.**
- `PopulateObject` builds a `Dictionary<JsonProperty, PropertyPresence>`, with every contract property starting as `PropertyPresence.None` (lines 2467-2469).
- It does this only if `contract.HasRequiredOrDefaultValueProperties` is true or the serializer's `DefaultValueHandling` has the `Populate` flag. Otherwise the dictionary is `null` and nothing is enforced.
- The constructor-based path computes the same flag at line 2065.

**2. Presence is recorded while reading.**
- `SetPropertyPresence(reader, property, requiredProperties)` (line 2726) updates the dictionary entry (line 2747) as each property is read.
- I read only its signature and the final assignment (lines 2726-2747).
- A property that appears in the JSON is marked as present. A property whose JSON value is `null` is marked as `PropertyPresence.Null`.

**3. Enforcement runs after the object's closing token.**
- Once the read loop ends, the code throws an "Unexpected end" exception if it never finished (lines 2576-2579).
- Otherwise it calls `EndProcessProperty(..., presence, true)` for every entry in the dictionary (lines 2581-2590), before `OnDeserialized` (line 2592).

**4. `EndProcessProperty` decides whether to throw (lines 2671-2724).**
- It does nothing unless the presence is `None` or `Null` (line 2673).
- The effective requirement is `property.Ignored ? Required.Default : property._required ?? contract.ItemRequired ?? Required.Default` (line 2677).
  - A property-level setting wins over the contract's `ItemRequired`.
  - Ignored properties are never required.
- The property is missing (`None`, lines 2681-2685):
  - `Required.Always` or `Required.AllowNull` throws `JsonSerializationException` with "Required property '{0}' not found in JSON."
  - Otherwise, if `setDefaultValue` is true and `DefaultValueHandling.Populate` applies to a writable property, the default value is set (lines 2687-2698).
- The property is present but `null` (`Null`, lines 2700-2709):
  - `Required.Always` throws "Required property '{0}' expects a value but got null."
  - `Required.DisallowNull` throws "Property '{0}' expects a non-null value."
  - `Required.AllowNull` permits `null`.

**5. Error handling.** The check sits inside a `try`. Any exception goes to `IsErrorHandled`, so an `OnError` callback or `Error` event handler can swallow it by calling `HandleError`. Otherwise it is rethrown (lines 2712-2721).

**Not verified:**
- I didn't open where `HasRequiredOrDefaultValueProperties` is computed.
- I didn't open the `JsonProperty.Required` setter or `JsonRequiredAttribute`.
- I didn't read the constructor-path call at line 2280, other than seeing that it also calls `EndProcessProperty`.