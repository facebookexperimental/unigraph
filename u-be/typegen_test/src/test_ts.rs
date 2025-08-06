use crate::shared::format_types;
use crate::shared::gen_config;
use crate::shared::get_all_declarations;

#[test]
fn test_typescript_generation() {
    let config = gen_config();
    let files = get_all_declarations()
        .iter()
        .filter_map(|decl| config.make_typescript_file(decl.clone()).unwrap())
        .collect::<Vec<_>>();

    k9::snapshot!(
        format_types(&files),
        r#"
---------------- ./ts/TSPrefixAddress.ts

/* ts header */

/** Simple address struct for testing */
export interface TSTypeAddress {
  /** Street address */
  street: string;
  city: string;
  zip_code: number;
  coordinates: [number, number, number];
  typegen_as: number;
  string_list: string[];
  maybe_flag?: boolean | undefined;
}
---------------- ./ts/TSPrefixPerson.ts

/* ts header */

import type { TSTypeAddress } from './TSPrefixAddress.ts';

/** Person struct that references Address */
export interface TSTypePerson {
  name: string;
  age: number;
  address: Address;
}
---------------- ./ts/TSPrefixUser.ts

/* ts header */

/** Test struct with optional fields */
export interface TSTypeUser {
  id: number;
  email: string;
  profile?: string | undefined;
  verified: boolean;
  tags: { [key: string]: string };
  metadata: { [key: string]: boolean };
}
---------------- ./ts/TSPrefixPoint.ts

/* ts header */

/** Test tuple struct */
export type TSTypePoint = [number, number];
---------------- ./ts/TSPrefixUnit.ts

/* ts header */

/** Test unit struct */
export type TSTypeUnit = null;
---------------- ./ts/TSPrefixWrappedString.ts

/* ts header */

/**
 * This is a wrapper for a String type. The type
 * should be transparent in the generated code and point directly to the string type.
 */
export type TSTypeWrappedString = string;
---------------- ./ts/TSPrefixAnimal.ts

/* ts header */

/** Simple enum with unit variants */
export type TSTypeAnimal = "Cat" | "Dog" | "Fish";
---------------- ./ts/TSPrefixShape.ts

/* ts header */

/** Complex enum with different variant types */
export type TSTypeShape =
  /** Circle with radius */
  { "Circle": number } |
  /** Rectangle with width and height */
  { "Rectangle": [number, number] } |
  /** Point with coordinates */
  { "Point": { x: number, y: number, z: number } };
"#
    );
}
