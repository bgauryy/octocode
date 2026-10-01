Required members are enforced after the JSON object has been read. The reader records whether each property was seen, then checks each property's `Required` setting at the end. All paths are in `Src/Newtonsoft.Json/Serialization/`.

**1. Where `Required` comes from (`DefaultContractResolver.cs`)**
- `[JsonRequired]` sets `property._required = Required.Always` (`:1587`, attribute lookup at `:1519`).
- `[DataMember(IsRequired = true)]` sets `Required.AllowNull` (`:1577`).
- `[JsonObject(ItemRequired = …)]` sets `contract.ItemRequired` (`:364`).

**2. Tracking is only switched on when needed**
- `JsonObjectContract.HasRequiredOrDefaultValueProperties` (`JsonObjectContract.cs:143-168`) is true if `ItemRequired` is not `Default`. It is also true if any property has `Required != Default` or `DefaultValueHandling.Populate`.
- `PopulateObject` only builds the presence dictionary when that flag is true (or the serializer has `DefaultValueHandling.Populate`). Each property starts as `PropertyPresence.None` (`JsonSerializerInternalReader.cs:2467-2468`).

**3. Recording what was seen**
- When a property is read, `SetPropertyPresence` (`JsonSerializerInternalReader.cs:2726-2746`) stores `Null` or `Value` for it. `Null` covers a JSON `null`, `undefined`, or an empty string that is coerced to null. Anything else is `Value`. The calls are at `:2526` and `:2543`.

**4. Enforcement**
- After the read loop and before `OnDeserialized`, the code calls `EndProcessProperty` for every property (`JsonSerializerInternalReader.cs:2581-2589`).
- `EndProcessProperty` (`:2671-2723`) resolves the setting as `property.Ignored ? Default : property._required ?? contract.ItemRequired ?? Default` (`:2677`).
- Then it checks the presence:
  - **Missing:** if the setting is `AllowNull` or `Always`, it throws `JsonSerializationException` with "Required property '{0}' not found in JSON." (`:2682-2684`).
  - **Present but null:** if the setting is `Always`, it throws "Required property '{0}' expects a value but got null." (`:2701-2703`). If it is `DisallowNull`, it throws "Property '{0}' expects a non-null value." (`:2705`).
- The exceptions pass through `IsErrorHandled` (`:2712`). If an error handler handles the error, the serializer skips past it instead of throwing.

**5. Constructor-based creation**
- For objects built through a parameterized constructor, the same `EndProcessProperty` is called for each property context, with presence recorded in `CreatorPropertyContext.Presence` (`:2048`, `:2091-2121`, `:2280-2288`).

**Uncertainty:** I did not read the code between `:2100` and `:2270` in detail, so the constructor path is summarised from the lines listed above.