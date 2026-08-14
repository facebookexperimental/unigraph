// Copyright (c) Meta Platforms, Inc. and affiliates.

use typegen::Lang;

use crate::shared::format_types;
use crate::shared::gen_config;
use crate::shared::get_all_declarations;

#[test]
fn test_typescript_generation() {
    let config = gen_config();
    let files = get_all_declarations()
        .iter()
        .filter_map(|decl| config.make_file(decl.clone(), Lang::TypeScript).unwrap())
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
  tags: string[];
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

export type TSTypeShapeVariants = "Circle" | "Rectangle" | "Point";
---------------- ./ts/TSPrefixHttpMethod.ts

/* ts header */

/** Simple enum with multi-word CamelCase variants to test SCREAMING_SNAKE_CASE conversion */
export type TSTypeHttpMethod = "GetRequest" | "PostRequest" | "DeleteAll" | "XMLParser" | "SimpleA";
---------------- ./ts/TSPrefixOverrideTest.ts

/* ts header */

// Test struct with type overrides
export type TSTypeOverrideTest = () => void;

---------------- ./ts/TSPrefixSkipTest.ts

/* ts header */

/** Test struct that skips generation for Hack and Flow */
export interface TSTypeSkipTest {
  value: number;
}
---------------- ./ts/TSPrefixSkipAndOverrideTest.ts

/* ts header */

// Test struct that combines skips and overrides
export type TSTypeSkipAndOverrideTest = string;

---------------- ./ts/TSPrefixTimelines.ts

/* ts header */

/** Well-known timeline identifiers. */
export const TSTypeTimelines = {
  /** The main timeline. */
  MY_TIMELINE: "timeline-123",
  OTHER_TIMELINE: "timeline-456",
} as const;

export type TSTypeTimelines = (typeof TSTypeTimelines)[keyof typeof TSTypeTimelines];

---------------- ./ts/TSPrefixTrickyConsts.ts

/* ts header */

/** Values that need escaping in one or more target languages. */
export const TSTypeTrickyConsts = {
  /** Double quotes and backslashes. */
  QUOTED: "say \\"hi\\" \\\\ bye",
  /** Hack interpolates `$name` inside double-quoted strings. */
  DOLLAR: "{$notAVariable}",
  APOSTROPHE: "it's",
  NEWLINE: "line1\
line2",
} as const;

export type TSTypeTrickyConsts = (typeof TSTypeTrickyConsts)[keyof typeof TSTypeTrickyConsts];

---------------- ./ts/TSPrefixPartialConsts.ts

/* ts header */

/** Const group that opts out of Flow and overrides the Hack output. */
export const TSTypePartialConsts = {
  ONLY_SOME_LANGUAGES: "value",
} as const;

export type TSTypePartialConsts = (typeof TSTypePartialConsts)[keyof typeof TSTypePartialConsts];

---------------- ./ts/TSPrefixFlowOverriddenConsts.ts

/* ts header */

/**
 * Const group whose Flow output is overridden to a plain type alias.
 * 
 * An override emits no runtime values, so this one stays a `.js.flow`
 * declaration file rather than becoming a `.js` module.
 */
export const TSTypeFlowOverriddenConsts = {
  SOME_VALUE: "value",
} as const;

export type TSTypeFlowOverriddenConsts = (typeof TSTypeFlowOverriddenConsts)[keyof typeof TSTypeFlowOverriddenConsts];

"#
    );
}
