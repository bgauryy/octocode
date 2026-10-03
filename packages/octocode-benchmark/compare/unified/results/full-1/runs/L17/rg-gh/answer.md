**Short answer:** Newtonsoft.Json enforces required members after it has read the whole JSON object. It tracks which members appeared in the JSON. Then, for each member that was missing or null, `EndProcessProperty` throws a `JsonSerializationException` if the member's resolved `Required` setting says it must be present or non-null. Nothing is checked while the members are being read.

All paths below are under `Src/Newtonsoft.Json/`, at commit 52fa3aef1f.

**1. Where "required" is set** (`Serialization/DefaultContractResolver.cs`)
- `[JsonRequired]` sets `property._required = Required.Always` (line 1587, attribute read at line 1519).
- `[DataMember(IsRequired = true)]` sets `Required.AllowNull`. Otherwise it sets `Required.Default` (line 1577).
- `[JsonObject(ItemRequired = ...)]` is stored as `contract.ItemRequired` (line 364).
- `[JsonProperty(Required = ...)]` writes to `JsonProperty._required` (`Serialization/JsonProperty.cs:46`, `JsonPropertyAttribute.cs:165`).

**2. Presence tracking while reading** (`Serialization/JsonSerializerInternalReader.cs`)
- Tracking is only switched on when it is needed. `propertiesPresence` is created only if `contract.HasRequiredOrDefaultValueProperties`, or if `DefaultValueHandling.Populate` is set (line 2467).
- The dictionary starts with every contract property marked `PropertyPresence.None` (line 2468).
- As each JSON property is read, `SetPropertyPresence` is called (lines 2526 and 2543). It records `Value` or `Null`. The enum is at line 55.
- Constructor-based creation does the same thing with `CreatorPropertyContext.Presence` (lines 2048–2121).

**3. The check itself** (`EndProcessProperty`, lines 2671–2717)
- Once the object's closing token is reached, the reader loops over the presence dictionary and calls `EndProcessProperty` for each property (lines 2583–2588). The constructor path does the same at lines 2271–2287.
- `EndProcessProperty` resolves the setting as `property.Ignored ? Default : property._required ?? contract.ItemRequired ?? Default` (line 2677).
- If the property was **absent** (`None`) and the setting is `Always` or `AllowNull`, it throws `"Required property '{0}' not found in JSON."` (lines 2682–2685).
- If the property was present with a **null** value:
  - `Always` throws `"Required property '{0}' expects a value but got null."` (lines 2701–2704).
  - `DisallowNull` throws `"Property '{0}' expects a non-null value."` (lines 2705–2708).
- These exceptions go through `IsErrorHandled`. If an error handler such as `[OnError]` or the serializer's `Error` event handles the exception, the reader skips ahead and carries on. Otherwise the exception propagates (lines 2711–2720).

**Related but separate:** `MissingMemberHandling.Error` (lines 2382 and 2505) rejects JSON members that have no matching property on the type. It does not enforce required members.

**Uncertainty:** I did not read the `SetPropertyPresence` body or the `HasRequiredOrDefaultValueProperties` computation. I'm relying on their call sites and names for how they behave.