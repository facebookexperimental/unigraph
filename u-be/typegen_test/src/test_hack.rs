// Copyright (c) Meta Platforms, Inc. and affiliates.

use typegen::Lang;

use crate::shared::format_types;
use crate::shared::gen_config;
use crate::shared::get_all_declarations;

#[test]
fn test_flow_generation() {
    let config = gen_config();
    let files = get_all_declarations()
        .iter()
        .filter_map(|decl| config.make_file(decl.clone(), Lang::Hack).unwrap())
        .collect::<Vec<_>>();

    k9::snapshot!(
        format_types(&files),
        r#"
---------------- ./hack/HackPrefixAddress.php

<?hh
/* hack header */

// Simple address struct for testing
type HackTypeAddress = shape(
  // Street address
  'street' => string,
  'city' => string,
  'zip_code' => int,
  'coordinates' => vec<float>,
  'typegen_as' => int,
  'string_list' => vec<string>,
  ?'maybe_flag' => ?bool,
  'tags' => keyset<string>,
);

---------------- ./hack/HackPrefixPerson.php

<?hh
/* hack header */

// Person struct that references Address
type HackTypePerson = shape(
  'name' => string,
  'age' => int,
  'address' => HackTypeAddress,
);

---------------- ./hack/HackPrefixUser.php

<?hh
/* hack header */

// Test struct with optional fields
type HackTypeUser = shape(
  'id' => int,
  'email' => string,
  ?'profile' => ?string,
  'verified' => bool,
  'tags' => dict<string, string>,
  'metadata' => dict<string, bool>,
);

---------------- ./hack/HackPrefixPoint.php

<?hh
/* hack header */

// Test tuple struct
type HackTypePoint = (float, float);

---------------- ./hack/HackPrefixUnit.php

<?hh
/* hack header */

// Test unit struct
type HackTypeUnit = null;

---------------- ./hack/HackPrefixWrappedString.php

<?hh
/* hack header */

// This is a wrapper for a String type. The type
// should be transparent in the generated code and point directly to the string type.
type HackTypeWrappedString = string;

---------------- ./hack/HackPrefixAnimal.php

<?hh
/* hack header */

// Simple enum with unit variants
enum HackTypeAnimal: string as string {
  // A cat
  CAT = "Cat";
  // A dog
  DOG = "Dog";
  // A fish
  FISH = "Fish";
}

---------------- ./hack/HackPrefixShape.php

<?hh
/* hack header */

// Complex enum with different variant types
type HackTypeShape = shape(
  // Circle with radius
  ?'Circle' => ?float,
  // Rectangle with width and height
  ?'Rectangle' => ?(float, float),
  // Point with coordinates
  ?'Point' => ?shape(
    'x' => float,
    'y' => float,
    'z' => float,
  ),
);

enum HackTypeShapeVariant: string as string {
  CIRCLE = "Circle";
  RECTANGLE = "Rectangle";
  POINT = "Point";
}

---------------- ./hack/HackPrefixHttpMethod.php

<?hh
/* hack header */

// Simple enum with multi-word CamelCase variants to test SCREAMING_SNAKE_CASE conversion
enum HackTypeHttpMethod: string as string {
  GET_REQUEST = "GetRequest";
  POST_REQUEST = "PostRequest";
  DELETE_ALL = "DeleteAll";
  XML_PARSER = "XMLParser";
  SIMPLE_A = "SimpleA";
}

---------------- ./hack/HackPrefixOverrideTest.php

<?hh
/* hack header */

// Test struct with type overrides
type HackTypeOverrideTest = null;

---------------- ./hack/HackPrefixSkipAndOverrideTest.php

<?hh
/* hack header */

// Test struct that combines skips and overrides
type HackTypeSkipAndOverrideTest = shape(
  'data' => int,
);

---------------- ./hack/HackPrefixTimelines.php

<?hh
/* hack header */

// Well-known timeline identifiers.
enum HackTypeTimelines: string as string {
  // The main timeline.
  MY_TIMELINE = "timeline-123";
  OTHER_TIMELINE = "timeline-456";
}

---------------- ./hack/HackPrefixTrickyConsts.php

<?hh
/* hack header */

// Values that need escaping in one or more target languages.
enum HackTypeTrickyConsts: string as string {
  // Double quotes and backslashes.
  QUOTED = "say \\"hi\\" \\\\ bye";
  // Hack interpolates `$name` inside double-quoted strings.
  DOLLAR = "{\\$notAVariable}";
  APOSTROPHE = "it's";
  NEWLINE = "line1\
line2";
}

---------------- ./hack/HackPrefixPartialConsts.php

<?hh
/* hack header */

// Const group that opts out of Flow and overrides the Hack output.
type HackTypePartialConsts = string;

---------------- ./hack/HackPrefixFlowOverriddenConsts.php

<?hh
/* hack header */

// Const group whose Flow output is overridden to a plain type alias.
// 
// An override emits no runtime values, so this one stays a `.js.flow`
// declaration file rather than becoming a `.js` module.
enum HackTypeFlowOverriddenConsts: string as string {
  SOME_VALUE = "value";
}

"#
    );
}
