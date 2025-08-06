use std::collections::BTreeMap;
use std::collections::HashMap;

use typegen::TypeGen;

type StringAlias = String;

pub struct NotTypeGennable(pub u32);

/// Simple address struct for testing
#[derive(TypeGen)]
pub struct Address {
    /// Street address
    pub street: StringAlias,
    pub city: String,
    pub zip_code: u32,
    pub coordinates: [f32; 3],
    #[typegen(as = "u32")]
    pub typegen_as: NotTypeGennable,
    #[typegen(as = "Vec<String>")]
    pub string_list: NotTypeGennable,
    #[typegen(as = "Option<bool>")]
    pub maybe_flag: NotTypeGennable,
}

/// Person struct that references Address
#[derive(TypeGen)]
pub struct Person {
    pub name: String,
    pub age: u32,
    pub address: Address,
}

/// Test struct with optional fields
#[derive(TypeGen)]
pub struct User {
    pub id: u64,
    pub email: String,
    pub profile: Option<String>,
    pub verified: bool,
    pub tags: HashMap<StringAlias, String>,
    pub metadata: BTreeMap<String, bool>,
}

/// This is a wrapper for a String type. The type
/// should be transparent in the generated code and point directly to the string type.
#[derive(TypeGen)]
pub struct WrappedString(pub StringAlias);

/// Test tuple struct
#[derive(TypeGen)]
pub struct Point(pub f64, pub f64);

/// Test unit struct
#[derive(TypeGen)]
pub struct Unit;

/// Simple enum with unit variants
#[derive(TypeGen)]
pub enum Animal {
    /// A cat
    Cat,
    /// A dog
    Dog,
    /// A fish
    Fish,
}

/// Complex enum with different variant types
#[derive(TypeGen)]
pub enum Shape {
    /// Circle with radius
    Circle(f64),
    /// Rectangle with width and height
    Rectangle(f64, f64),
    /// Point with coordinates
    Point {
        x: f64,
        y: f64,
        #[typegen(as = "f64")]
        z: NotTypeGennable,
    },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use typegen::FlowConfig;
    use typegen::SharedConfig;
    use typegen::TypeGenConfig;
    use typegen::TypeGenDeclTrait;
    use typegen::TypeGenFile;
    use typegen::TypeGenGeneratedType;
    use typegen::TypeScriptConfig;

    use super::*;

    fn get_all_declarations() -> Vec<TypeGenGeneratedType> {
        vec![
            Address::to_type_decl(),
            Person::to_type_decl(),
            User::to_type_decl(),
            Point::to_type_decl(),
            Unit::to_type_decl(),
            WrappedString::to_type_decl(),
            Animal::to_type_decl(),
            Shape::to_type_decl(),
        ]
    }

    fn gen_config() -> TypeGenConfig {
        TypeGenConfig {
            typescript: Some(TypeScriptConfig {
                shared_config: SharedConfig {
                    export_path: Some("./ts".to_string()),
                    header: Some("/* ts header */".to_string()),
                },
            }),
            flow: Some(FlowConfig {
                shared_config: SharedConfig {
                    export_path: Some("./flow".to_string()),
                    header: Some("/* flow header */".to_string()),
                },
            }),
            config_file_path: PathBuf::from("typegen_config.json"),
        }
    }

    fn format_types(files: &[TypeGenFile]) -> String {
        files
            .iter()
            .map(|file| {
                format!(
                    "---------------- {}\n\n{}",
                    file.path.display(),
                    file.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

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
---------------- ./ts/Address.ts

/* ts header */

/** Simple address struct for testing */
export interface Address {
  /** Street address */
  street: string;
  city: string;
  zip_code: number;
  coordinates: [number, number, number];
  typegen_as: number;
  string_list: string[];
  maybe_flag?: boolean | undefined;
}
---------------- ./ts/Person.ts

/* ts header */

import type { Address } from './Address.ts';

/** Person struct that references Address */
export interface Person {
  name: string;
  age: number;
  address: Address;
}
---------------- ./ts/User.ts

/* ts header */

/** Test struct with optional fields */
export interface User {
  id: number;
  email: string;
  profile?: string | undefined;
  verified: boolean;
  tags: { [key: string]: string };
  metadata: { [key: string]: boolean };
}
---------------- ./ts/Point.ts

/* ts header */

/** Test tuple struct */
export type Point = [number, number];
---------------- ./ts/Unit.ts

/* ts header */

/** Test unit struct */
export type Unit = null;
---------------- ./ts/WrappedString.ts

/* ts header */

/**
 * This is a wrapper for a String type. The type
 * should be transparent in the generated code and point directly to the string type.
 */
export type WrappedString = string;
---------------- ./ts/Animal.ts

/* ts header */

/** Simple enum with unit variants */
export type Animal = "Cat" | "Dog" | "Fish";
---------------- ./ts/Shape.ts

/* ts header */

/** Complex enum with different variant types */
export type Shape = 
  /** Circle with radius */
  { "Circle": number } | 
  /** Rectangle with width and height */
  { "Rectangle": [number, number] } | 
  /** Point with coordinates */
  { "Point": { x: number, y: number, z: number } };
"#
        );
    }

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
---------------- ./flow/Address.js.flow

/* flow header */

// Simple address struct for testing
export type Address = {
  // Street address
  street: string,
  city: string,
  zip_code: number,
  coordinates: [number, number, number],
  typegen_as: number,
  string_list: Array<string>,
  maybe_flag?: ?boolean,
};
---------------- ./flow/Person.js.flow

/* flow header */

import type { Address } from './Address.js.flow';

// Person struct that references Address
export type Person = {
  name: string,
  age: number,
  address: Address,
};
---------------- ./flow/User.js.flow

/* flow header */

// Test struct with optional fields
export type User = {
  id: number,
  email: string,
  profile?: ?string,
  verified: boolean,
  tags: { [key: string]: string },
  metadata: { [key: string]: boolean },
};
---------------- ./flow/Point.js.flow

/* flow header */

// Test tuple struct
export type Point = [number, number];
---------------- ./flow/Unit.js.flow

/* flow header */

// Test unit struct
export type Unit = null;
---------------- ./flow/WrappedString.js.flow

/* flow header */

// This is a wrapper for a String type. The type
// should be transparent in the generated code and point directly to the string type.
export type WrappedString = string;
---------------- ./flow/Animal.js.flow

/* flow header */

// Simple enum with unit variants
export type Animal = "Cat" | "Dog" | "Fish";
---------------- ./flow/Shape.js.flow

/* flow header */

// Complex enum with different variant types
export type Shape = 
  // Circle with radius
  { "Circle": number } | 
  // Rectangle with width and height
  { "Rectangle": [number, number] } | 
  // Point with coordinates
  { "Point": { x: number, y: number, z: number } };
"#
        );
    }

    #[test]
    fn test_type_declarations() {
        let output = get_all_declarations()
            .into_iter()
            .map(|d| format!("{d:#?}"))
            .collect::<Vec<_>>()
            .join("\n\n");

        let sanitized = output
            .lines()
            .map(|line| {
                if line.contains("file_path") {
                    let no_path = line.split(":").next().unwrap();
                    format!("{no_path}: <SANITIZED>")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        k9::snapshot!(
            sanitized,
            r#"
TypeGenGeneratedType {
    type_name: "Address",
    docs: Some(
        "Simple address struct for testing",
    ),
    file_path: <SANITIZED>
    declaration: StructDecl(
        StructDecl {
            fields: [
                FieldDeclaration {
                    field_name: "street",
                    type_ref: Primitive(
                        String,
                    ),
                    docs: Some(
                        "Street address",
                    ),
                },
                FieldDeclaration {
                    field_name: "city",
                    type_ref: Primitive(
                        String,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "zip_code",
                    type_ref: Primitive(
                        U32,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "coordinates",
                    type_ref: Array {
                        element_type: Primitive(
                            F32,
                        ),
                        size: 3,
                    },
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "typegen_as",
                    type_ref: Primitive(
                        U32,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "string_list",
                    type_ref: Vec(
                        Primitive(
                            String,
                        ),
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "maybe_flag",
                    type_ref: Option(
                        Primitive(
                            Bool,
                        ),
                    ),
                    docs: None,
                },
            ],
        },
    ),
}

TypeGenGeneratedType {
    type_name: "Person",
    docs: Some(
        "Person struct that references Address",
    ),
    file_path: <SANITIZED>
    declaration: StructDecl(
        StructDecl {
            fields: [
                FieldDeclaration {
                    field_name: "name",
                    type_ref: Primitive(
                        String,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "age",
                    type_ref: Primitive(
                        U32,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "address",
                    type_ref: TypeReference(
                        "Address",
                    ),
                    docs: None,
                },
            ],
        },
    ),
}

TypeGenGeneratedType {
    type_name: "User",
    docs: Some(
        "Test struct with optional fields",
    ),
    file_path: <SANITIZED>
    declaration: StructDecl(
        StructDecl {
            fields: [
                FieldDeclaration {
                    field_name: "id",
                    type_ref: Primitive(
                        U64,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "email",
                    type_ref: Primitive(
                        String,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "profile",
                    type_ref: Option(
                        Primitive(
                            String,
                        ),
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "verified",
                    type_ref: Primitive(
                        Bool,
                    ),
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "tags",
                    type_ref: Map {
                        key: Primitive(
                            String,
                        ),
                        value: Primitive(
                            String,
                        ),
                    },
                    docs: None,
                },
                FieldDeclaration {
                    field_name: "metadata",
                    type_ref: Map {
                        key: Primitive(
                            String,
                        ),
                        value: Primitive(
                            Bool,
                        ),
                    },
                    docs: None,
                },
            ],
        },
    ),
}

TypeGenGeneratedType {
    type_name: "Point",
    docs: Some(
        "Test tuple struct",
    ),
    file_path: <SANITIZED>
    declaration: TupleStructDecl(
        TupleStructDecl {
            fields: [
                Primitive(
                    F64,
                ),
                Primitive(
                    F64,
                ),
            ],
        },
    ),
}

TypeGenGeneratedType {
    type_name: "Unit",
    docs: Some(
        "Test unit struct",
    ),
    file_path: <SANITIZED>
    declaration: Null,
}

TypeGenGeneratedType {
    type_name: "WrappedString",
    docs: Some(
        "This is a wrapper for a String type. The type\
should be transparent in the generated code and point directly to the string type.",
    ),
    file_path: <SANITIZED>
    declaration: TupleStructDecl(
        TupleStructDecl {
            fields: [
                Primitive(
                    String,
                ),
            ],
        },
    ),
}

TypeGenGeneratedType {
    type_name: "Animal",
    docs: Some(
        "Simple enum with unit variants",
    ),
    file_path: <SANITIZED>
    declaration: EnumDecl(
        EnumDecl {
            variants: [
                Unit {
                    name: "Cat",
                    docs: Some(
                        "A cat",
                    ),
                },
                Unit {
                    name: "Dog",
                    docs: Some(
                        "A dog",
                    ),
                },
                Unit {
                    name: "Fish",
                    docs: Some(
                        "A fish",
                    ),
                },
            ],
        },
    ),
}

TypeGenGeneratedType {
    type_name: "Shape",
    docs: Some(
        "Complex enum with different variant types",
    ),
    file_path: <SANITIZED>
    declaration: EnumDecl(
        EnumDecl {
            variants: [
                Newtype {
                    name: "Circle",
                    docs: Some(
                        "Circle with radius",
                    ),
                    field_type: Primitive(
                        F64,
                    ),
                },
                Tuple {
                    name: "Rectangle",
                    docs: Some(
                        "Rectangle with width and height",
                    ),
                    fields: [
                        Primitive(
                            F64,
                        ),
                        Primitive(
                            F64,
                        ),
                    ],
                },
                Struct {
                    name: "Point",
                    docs: Some(
                        "Point with coordinates",
                    ),
                    fields: [
                        FieldDeclaration {
                            field_name: "x",
                            type_ref: Primitive(
                                F64,
                            ),
                            docs: None,
                        },
                        FieldDeclaration {
                            field_name: "y",
                            type_ref: Primitive(
                                F64,
                            ),
                            docs: None,
                        },
                        FieldDeclaration {
                            field_name: "z",
                            type_ref: Primitive(
                                F64,
                            ),
                            docs: None,
                        },
                    ],
                },
            ],
        },
    ),
}
"#
        );
    }
}
