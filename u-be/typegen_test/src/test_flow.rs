use crate::shared::format_types;
use crate::shared::gen_config;
use crate::shared::get_all_declarations;

#[test]
fn test_flow_generation() {
    let config = gen_config();
    let files = get_all_declarations()
        .iter()
        .filter_map(|decl| config.make_flow_file(decl.clone()).unwrap())
        .collect::<Vec<_>>();

    k9::snapshot!(
        format_types(&files),
        r#"
---------------- ./flow/FlowPrefixAddress.js.flow

/* flow header */

/** Simple address struct for testing */
export type FlowTypeAddress = {
  /** Street address */
  street: string,
  city: string,
  zip_code: number,
  coordinates: [number, number, number],
  typegen_as: number,
  string_list: Array<string>,
  maybe_flag?: ?boolean,
};
---------------- ./flow/FlowPrefixPerson.js.flow

/* flow header */

import type { FlowTypeAddress } from './FlowPrefixAddress.js.flow';

/** Person struct that references Address */
export type FlowTypePerson = {
  name: string,
  age: number,
  address: Address,
};
---------------- ./flow/FlowPrefixUser.js.flow

/* flow header */

/** Test struct with optional fields */
export type FlowTypeUser = {
  id: number,
  email: string,
  profile?: ?string,
  verified: boolean,
  tags: { [key: string]: string },
  metadata: { [key: string]: boolean },
};
---------------- ./flow/FlowPrefixPoint.js.flow

/* flow header */

/** Test tuple struct */
export type FlowTypePoint = [number, number];
---------------- ./flow/FlowPrefixUnit.js.flow

/* flow header */

/** Test unit struct */
export type FlowTypeUnit = null;
---------------- ./flow/FlowPrefixWrappedString.js.flow

/* flow header */

/**
 * This is a wrapper for a String type. The type
 * should be transparent in the generated code and point directly to the string type.
 */
export type FlowTypeWrappedString = string;
---------------- ./flow/FlowPrefixAnimal.js.flow

/* flow header */

/** Simple enum with unit variants */
export type FlowTypeAnimal = "Cat" | "Dog" | "Fish";
---------------- ./flow/FlowPrefixShape.js.flow

/* flow header */

/** Complex enum with different variant types */
export type FlowTypeShape =
  /** Circle with radius */
  { "Circle": number } |
  /** Rectangle with width and height */
  { "Rectangle": [number, number] } |
  /** Point with coordinates */
  { "Point": { x: number, y: number, z: number } };
---------------- ./flow/FlowPrefixOverrideTest.js.flow

/* flow header */

// Test struct with type overrides
export type FlowTypeOverrideTest = () => void;

"#
    );
}
