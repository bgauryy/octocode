# Protocol Buffers (.proto)

Source sample: `proto/protobuf-descriptor.proto`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 60347 | - | - |
| content-view | 18638 | 69.1% | 1.02 ms |
| applyMinification | 18810 | 68.8% | 0.947 ms |
| sync minify | 18810 | 68.8% | 0.913 ms |
| async minify | 18810 | 68.8% | 0.931 ms |
| symbols | 2919 | 95.2% | 0.118 ms |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```proto
// Protocol Buffers - Google's data interchange format
// Copyright 2008 Google LLC.  All rights reserved.
//
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file or at
// https://developers.google.com/open-source/licenses/bsd

// Author: kenton@google.com (Kenton Varda)
//  Based on original Protocol Buffers design by
//  Sanjay Ghemawat, Jeff Dean, and others.
//
// The messages in this file describe the definitions found in .proto files.
// A valid .proto file can be translated directly to a FileDescriptorProto
// without any other information (e.g. without reading its imports).

syntax = "proto2";

package google.protobuf;

option go_package = "google.golang.org/protobuf/types/descriptorpb";
option java_package = "com.google.protobuf";
option java_outer_classname = "DescriptorProtos";
option csharp_namespace = "Google.Protobuf.Reflection";
option objc_class_prefix = "GPB";
option cc_enable_arenas = true;

// descriptor.proto must be optimized for speed because reflection-based
// algorithms don't work during bootstrapping.
option optimize_for = SPEED;

// The protocol compiler can output a FileDescriptorSet containing the .proto
// files it parses.
mes

... [truncated 58547 chars] ...

e mutated.
      SET = 1;
      // An alias to the element is returned.
      ALIAS = 2;
    }
    optional Semantic semantic = 5;
  }
}

// Describes the 'visibility' of a symbol with respect to the proto import
// system. Symbols can only be imported when the visibility rules do not prevent
// it (ex: local symbols cannot be imported).  Visibility modifiers can only set
// on `message` and `enum` as they are the only types available to be referenced
// from other files.
enum SymbolVisibility {
  VISIBILITY_UNSET = 0;
  VISIBILITY_LOCAL = 1;
  VISIBILITY_EXPORT = 2;
}

```

## Content-View Excerpt

```proto
syntax = "proto2";

package google.protobuf;

option go_package = "google.golang.org/protobuf/types/descriptorpb";
option java_package = "com.google.protobuf";
option java_outer_classname = "DescriptorProtos";
option csharp_namespace = "Google.Protobuf.Reflection";
option objc_class_prefix = "GPB";
option cc_enable_arenas = true;

option optimize_for = SPEED;

message FileDescriptorSet {
  repeated FileDescriptorProto file = 1;

  extensions 536000000 [declaration = {
    number: 536000000
    type: ".buf.descriptor.v1.FileDescriptorSetExtension"
    full_name: ".buf.descriptor.v1.buf_file_descriptor_set_extension"
  }];
}

enum Edition {

  EDITION_UNKNOWN = 0;

  EDITION_LEGACY = 900;

  EDITION_PROTO2 = 998;
  EDITION_PROTO3 = 999;

  EDITION_2023 = 1000;
  EDITION_2024 = 1001;
  EDITION_2026 = 1002;

  EDITION_UNSTABLE = 9999;

  EDITION_1_TEST_ONLY = 1;
  EDITION_2_TEST_ONLY = 2;
  EDITION_99997_TEST_ONLY = 99997;
  EDITION_99998_TEST_ONLY = 99998;
  EDITION_99999_TEST_ONLY = 99999;

  EDITION_MAX = 0x7FFFFFFF;
}

message FileDescriptorProto {
  optional string name = 1;
  optional string package = 2;

  repeated string dependency = 3;

  repeated int32 public_dependency = 10;

  repeated int32 weak_

... [truncated 16838 chars] ...

: ".buf.descriptor.v1.SourceCodeInfoExtension"
    full_name: ".buf.descriptor.v1.buf_source_code_info_extension"
  }];
}

message GeneratedCodeInfo {

  repeated Annotation annotation = 1;
  message Annotation {

    repeated int32 path = 1 [packed = true];

    optional string source_file = 2;

    optional int32 begin = 3;

    optional int32 end = 4;

    enum Semantic {

      NONE = 0;

      SET = 1;

      ALIAS = 2;
    }
    optional Semantic semantic = 5;
  }
}

enum SymbolVisibility {
  VISIBILITY_UNSET = 0;
  VISIBILITY_LOCAL = 1;
  VISIBILITY_EXPORT = 2;
}
```

## Apply Minification Excerpt

```proto


syntax = "proto2";

package google.protobuf;

option go_package = "google.golang.org/protobuf/types/descriptorpb";
option java_package = "com.google.protobuf";
option java_outer_classname = "DescriptorProtos";
option csharp_namespace = "Google.Protobuf.Reflection";
option objc_class_prefix = "GPB";
option cc_enable_arenas = true;


option optimize_for = SPEED;


message FileDescriptorSet {
  repeated FileDescriptorProto file = 1;


  extensions 536000000 [declaration = {
    number: 536000000
    type: ".buf.descriptor.v1.FileDescriptorSetExtension"
    full_name: ".buf.descriptor.v1.buf_file_descriptor_set_extension"
  }];
}


enum Edition {

  EDITION_UNKNOWN = 0;


  EDITION_LEGACY = 900;


  EDITION_PROTO2 = 998;
  EDITION_PROTO3 = 999;


  EDITION_2023 = 1000;
  EDITION_2024 = 1001;
  EDITION_2026 = 1002;


  EDITION_UNSTABLE = 9999;


  EDITION_1_TEST_ONLY = 1;
  EDITION_2_TEST_ONLY = 2;
  EDITION_99997_TEST_ONLY = 99997;
  EDITION_99998_TEST_ONLY = 99998;
  EDITION_99999_TEST_ONLY = 99999;


  EDITION_MAX = 0x7FFFFFFF;
}


message FileDescriptorProto {
  optional string name = 1;
  optional string package = 2;


  repeated string dependency = 3;

  repeated int32 public_dependency = 10;


  repea

... [truncated 17010 chars] ...

descriptor.v1.SourceCodeInfoExtension"
    full_name: ".buf.descriptor.v1.buf_source_code_info_extension"
  }];
}


message GeneratedCodeInfo {


  repeated Annotation annotation = 1;
  message Annotation {


    repeated int32 path = 1 [packed = true];


    optional string source_file = 2;


    optional int32 begin = 3;


    optional int32 end = 4;


    enum Semantic {

      NONE = 0;

      SET = 1;

      ALIAS = 2;
    }
    optional Semantic semantic = 5;
  }
}


enum SymbolVisibility {
  VISIBILITY_UNSET = 0;
  VISIBILITY_LOCAL = 1;
  VISIBILITY_EXPORT = 2;
}
```

## Sync Minify Excerpt

```proto


syntax = "proto2";

package google.protobuf;

option go_package = "google.golang.org/protobuf/types/descriptorpb";
option java_package = "com.google.protobuf";
option java_outer_classname = "DescriptorProtos";
option csharp_namespace = "Google.Protobuf.Reflection";
option objc_class_prefix = "GPB";
option cc_enable_arenas = true;


option optimize_for = SPEED;


message FileDescriptorSet {
  repeated FileDescriptorProto file = 1;


  extensions 536000000 [declaration = {
    number: 536000000
    type: ".buf.descriptor.v1.FileDescriptorSetExtension"
    full_name: ".buf.descriptor.v1.buf_file_descriptor_set_extension"
  }];
}


enum Edition {

  EDITION_UNKNOWN = 0;


  EDITION_LEGACY = 900;


  EDITION_PROTO2 = 998;
  EDITION_PROTO3 = 999;


  EDITION_2023 = 1000;
  EDITION_2024 = 1001;
  EDITION_2026 = 1002;


  EDITION_UNSTABLE = 9999;


  EDITION_1_TEST_ONLY = 1;
  EDITION_2_TEST_ONLY = 2;
  EDITION_99997_TEST_ONLY = 99997;
  EDITION_99998_TEST_ONLY = 99998;
  EDITION_99999_TEST_ONLY = 99999;


  EDITION_MAX = 0x7FFFFFFF;
}


message FileDescriptorProto {
  optional string name = 1;
  optional string package = 2;


  repeated string dependency = 3;

  repeated int32 public_dependency = 10;


  repea

... [truncated 17010 chars] ...

descriptor.v1.SourceCodeInfoExtension"
    full_name: ".buf.descriptor.v1.buf_source_code_info_extension"
  }];
}


message GeneratedCodeInfo {


  repeated Annotation annotation = 1;
  message Annotation {


    repeated int32 path = 1 [packed = true];


    optional string source_file = 2;


    optional int32 begin = 3;


    optional int32 end = 4;


    enum Semantic {

      NONE = 0;

      SET = 1;

      ALIAS = 2;
    }
    optional Semantic semantic = 5;
  }
}


enum SymbolVisibility {
  VISIBILITY_UNSET = 0;
  VISIBILITY_LOCAL = 1;
  VISIBILITY_EXPORT = 2;
}
```

## Async Minify Excerpt

```proto


syntax = "proto2";

package google.protobuf;

option go_package = "google.golang.org/protobuf/types/descriptorpb";
option java_package = "com.google.protobuf";
option java_outer_classname = "DescriptorProtos";
option csharp_namespace = "Google.Protobuf.Reflection";
option objc_class_prefix = "GPB";
option cc_enable_arenas = true;


option optimize_for = SPEED;


message FileDescriptorSet {
  repeated FileDescriptorProto file = 1;


  extensions 536000000 [declaration = {
    number: 536000000
    type: ".buf.descriptor.v1.FileDescriptorSetExtension"
    full_name: ".buf.descriptor.v1.buf_file_descriptor_set_extension"
  }];
}


enum Edition {

  EDITION_UNKNOWN = 0;


  EDITION_LEGACY = 900;


  EDITION_PROTO2 = 998;
  EDITION_PROTO3 = 999;


  EDITION_2023 = 1000;
  EDITION_2024 = 1001;
  EDITION_2026 = 1002;


  EDITION_UNSTABLE = 9999;


  EDITION_1_TEST_ONLY = 1;
  EDITION_2_TEST_ONLY = 2;
  EDITION_99997_TEST_ONLY = 99997;
  EDITION_99998_TEST_ONLY = 99998;
  EDITION_99999_TEST_ONLY = 99999;


  EDITION_MAX = 0x7FFFFFFF;
}


message FileDescriptorProto {
  optional string name = 1;
  optional string package = 2;


  repeated string dependency = 3;

  repeated int32 public_dependency = 10;


  repea

... [truncated 17010 chars] ...

descriptor.v1.SourceCodeInfoExtension"
    full_name: ".buf.descriptor.v1.buf_source_code_info_extension"
  }];
}


message GeneratedCodeInfo {


  repeated Annotation annotation = 1;
  message Annotation {


    repeated int32 path = 1 [packed = true];


    optional string source_file = 2;


    optional int32 begin = 3;


    optional int32 end = 4;


    enum Semantic {

      NONE = 0;

      SET = 1;

      ALIAS = 2;
    }
    optional Semantic semantic = 5;
  }
}


enum SymbolVisibility {
  VISIBILITY_UNSET = 0;
  VISIBILITY_LOCAL = 1;
  VISIBILITY_EXPORT = 2;
}
```

## Symbols

```txt
  16| syntax = "proto2";
  18| package google.protobuf;
  20| option go_package = "google.golang.org/protobuf/types/descriptorpb";
  21| option java_package = "com.google.protobuf";
  22| option java_outer_classname = "DescriptorProtos";
  23| option csharp_namespace = "Google.Protobuf.Reflection";
  24| option objc_class_prefix = "GPB";
  25| option cc_enable_arenas = true;
  29| option optimize_for = SPEED;
  33| message FileDescriptorSet {
  42| }
  45| enum Edition {
  82| }
  85| message FileDescriptorProto {
 129| }
 132| message DescriptorProto {
 167| }
 169| message ExtensionRangeOptions {
 224| }
 227| message FieldDescriptorProto {
 331| }
 334| message OneofDescriptorProto {
 337| }
 340| message EnumDescriptorProto {
 369| }
 372| message EnumValueDescriptorProto {
 377| }
 380| message ServiceDescriptorProto {
 388| }
 391| message MethodDescriptorProto {
 405| }
 439| message FileOptions {
 591| }
 593| message MessageOptions {
 680| }
 682| message FieldOptions {
 857| }
 859| message OneofOptions {
 875| }
 877| message EnumOptions {
 914| }
 916| message EnumValueOptions {
 950| }
 952| message ServiceOptions {
 980| }
 982| message MethodOptions {
1021| }
1029| message UninterpretedOption {
1049| }
1060| message FeatureSet {
1074|     edition_defaults = { edition: EDITION_LEGACY, value: "EXPLICIT" },
1075|     edition_defaults = { edition: EDITION_PROTO3, value: "IMPLICIT" },
1076|     edition_defaults = { edition: EDITION_2023, value: "EXPLICIT" }
1091|     edition_defaults = { edition: EDITION_LEGACY, value: "CLOSED" },
1092|     edition_defaults = { edition: EDITION_PROTO3, value: "OPEN" }
1107|     edition_defaults = { edition: EDITION_LEGACY, value: "EXPANDED" },
1108|     edition_defaults = { edition: EDITION_PROT

... [truncated 319 chars] ...

ACY_BEST_EFFORT" },
1157|     edition_defaults = { edition: EDITION_PROTO3, value: "ALLOW" }
1180|     edition_defaults = { edition: EDITION_LEGACY, value: "STYLE_LEGACY" },
1181|     edition_defaults = { edition: EDITION_2024, value: "STYLE2024" },
1182|     edition_defaults = { edition: EDITION_2026, value: "STYLE2026" }
1212|         edition_defaults = { edition: EDITION_LEGACY, value: "EXPORT_ALL" },
1213|         edition_defaults = { edition: EDITION_2024, value: "EXPORT_TOP_LEVEL" }
1244|     edition_defaults = { edition: EDITION_2026, value: "PROTO_LIMITS2026" }
1260|     declaration = { number: 1002, full_name: ".pb.go", type: ".pb.GoFeatures" },
1290| }
1296| message FeatureSetDefaults {
1322| }
1329| message SourceCodeInfo {
1465| }
1470| message GeneratedCodeInfo {
1503| }
1510| enum SymbolVisibility {
1514| }
```
