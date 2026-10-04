Newtonsoft.Json enforces required members at runtime during deserialization. It tracks which properties appeared in the JSON, then checks each one after the object has been read. A missing or null required property throws a `JsonSerializationException`. C# `required` keyword support is not covered here; I did not search for it.

**1. Where "required" comes from**
- `[JsonRequired]` sets `property._required = Required.Always` (`Serialization/DefaultContractResolver.cs:1587`, attribute lookup at `:1519`).
- `[DataMember(IsRequired = true)]` sets `Required.AllowNull` (`DefaultContractResolver.cs:1577`).
- `[JsonProperty(Required = ...)]` and `[JsonObject(ItemRequired = ...)]` also feed in. The class-level value is `contract.ItemRequired` (`DefaultContractResolver.cs:364`).
- A property's effective setting is `property._required ?? contract.ItemRequired ?? Required.Default`. An ignored property gets `Default` (`JsonSerializerInternalReader.cs:2677`).

**2. Presence tracking is only switched on when needed**
- `JsonObjectContract.HasRequiredOrDefaultValueProperties` is true if `ItemRequired` is not `Default`, or if any property has a non-default `Required` or `DefaultValueHandling.Populate` (`Serialization/JsonObjectContract.cs:143-160`).
- The reader builds a `Dictionary<JsonProperty, PropertyPresence>` starting at `None` (`JsonSerializerInternalReader.cs:2467-2468`). This is the normal path.
- As each property is read, `SetPropertyPresence` marks it `Null` or `Value` (`:2526`, `:2543`).
- The constructor-based path does the same with `trackPresence` and `context.Presence` (`:2065`, `:2103-2124`).

**3. The check after reading**
- At the end of `PopulateObject`, the reader loops over every tracked property and calls `EndProcessProperty` (`:2581-2589`).
- The constructor-based path does the same at `:2271-2287`.
- `EndProcessProperty` (`:2671-2716`) throws in these cases:
  - **Missing:** presence is `None` and the setting is `Always` or `AllowNull`. The message is "Required property '{0}' not found in JSON." (`:2682-2684`).
  - **Null with `Always`:** the message is "Required property '{0}' expects a value but got null." (`:2701-2703`).
  - **Null with `DisallowNull`:** the message is "Property '{0}' expects a non-null value." (`:2705`).
- These exceptions go through `IsErrorHandled`. If an error handler handles the exception, deserialization continues. Otherwise it is rethrown (`:2709-2715`).

**Uncertainty:** I read the `HasRequiredOrDefaultValueProperties` getter only up to line 160, so I did not see the rest of its loop body. Line numbers come from the pinned checkout.