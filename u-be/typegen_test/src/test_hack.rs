use crate::shared::format_types;
use crate::shared::gen_config;
use crate::shared::get_all_declarations;

#[test]
fn test_flow_generation() {
    let config = gen_config();
    let files = get_all_declarations()
        .iter()
        .filter_map(|decl| config.make_hack_file(decl.clone()).unwrap())
        .collect::<Vec<_>>();

    k9::snapshot!(
        format_types(&files),
        r#"
---------------- ./hack/HackPrefixAddress.hhi

<?hh
/* hack header */

<?hh
/**
 * Simple address struct for testing
 */
type Address = shape(
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

<?hh
use Address;

/**
 * Person struct that references Address
 */
type Person = shape(
  'name' => string,
  'age' => int,
  'address' => Address,
);

---------------- ./hack/HackPrefixUser.hhi

<?hh
/* hack header */

<?hh
/**
 * Test struct with optional fields
 */
type User = shape(
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

<?hh
/**
 * Test tuple struct
 */
type Point = (float, float);

---------------- ./hack/HackPrefixUnit.hhi

<?hh
/* hack header */

<?hh
/**
 * Test unit struct
 */
type Unit = null;

---------------- ./hack/HackPrefixWrappedString.hhi

<?hh
/* hack header */

<?hh
/**
 * This is a wrapper for a String type. The type
should be transparent in the generated code and point directly to the string type.
 */
type WrappedString = (string);

---------------- ./hack/HackPrefixAnimal.hhi

<?hh
/* hack header */

<?hh
/**
 * Simple enum with unit variants
 */
type Animal = 'Cat' | 'Dog' | 'Fish';

---------------- ./hack/HackPrefixShape.hhi

<?hh
/* hack header */

<?hh
/**
 * Complex enum with different variant types
 */
type Shape = shape('type' => 'Circle', 'data' => float) | shape('type' => 'Rectangle', 'data' => ('0' => float, '1' => float)) | shape('type' => 'Point', 'x' => float, 'y' => float, 'z' => float);

"#
    );
}
