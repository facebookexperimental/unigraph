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
---------------- ./hack/HackPrefixAddress.hhi

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
  'maybe_flag' => ?bool,
);

---------------- ./hack/HackPrefixPerson.hhi

<?hh
/* hack header */

// Person struct that references Address
type HackTypePerson = shape(
  'name' => string,
  'age' => int,
  'address' => HackTypeAddress,
);

---------------- ./hack/HackPrefixUser.hhi

<?hh
/* hack header */

// Test struct with optional fields
type HackTypeUser = shape(
  'id' => int,
  'email' => string,
  'profile' => ?string,
  'verified' => bool,
  'tags' => dict<string, string>,
  'metadata' => dict<string, bool>,
);

---------------- ./hack/HackPrefixPoint.hhi

<?hh
/* hack header */

// Test tuple struct
type HackTypePoint = (float, float);

---------------- ./hack/HackPrefixUnit.hhi

<?hh
/* hack header */

// Test unit struct
type HackTypeUnit = null;

---------------- ./hack/HackPrefixWrappedString.hhi

<?hh
/* hack header */

// This is a wrapper for a String type. The type
// should be transparent in the generated code and point directly to the string type.
type HackTypeWrappedString = string;

---------------- ./hack/HackPrefixAnimal.hhi

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

---------------- ./hack/HackPrefixShape.hhi

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

---------------- ./hack/HackPrefixOverrideTest.hhi

<?hh
/* hack header */

// Test struct with type overrides
type HackTypeOverrideTest = null;

---------------- ./hack/HackPrefixSkipAndOverrideTest.hhi

<?hh
/* hack header */

// Test struct that combines skips and overrides
type HackTypeSkipAndOverrideTest = shape(
  'data' => int,
);

"#
    );
}
