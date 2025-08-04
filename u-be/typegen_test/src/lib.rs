use std::collections::BTreeMap;
use std::collections::HashMap;

use typegen::TypeGen;

type StringAlias = String;

/// Simple address struct for testing
#[derive(TypeGen)]
pub struct Address {
    /// Street address
    pub street: StringAlias,
    pub city: String,
    pub zip_code: u32,
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
    Point { x: f64, y: f64 },
}

#[cfg(test)]
mod tests {
    use typegen::TypeGenDeclTrait;
    use typegen::TypeGenGeneratedType;

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

    #[test]
    fn test_typescript_generation() {
        let mut ts_output = String::new();
        for decl in get_all_declarations() {
            ts_output.push_str(&format!("---------------- {}\n\n", &decl.type_name));
            ts_output.push_str(&decl.export_typescript());
            ts_output.push_str("\n\n");
        }

        k9::snapshot!(
            ts_output.trim(),
            r#"
---------------- Address

/** Simple address struct for testing */
export interface Address {
  /** Street address */
  street: string;
  city: string;
  zip_code: number;
}

---------------- Person

import type { Address } from './Address.ts';

/** Person struct that references Address */
export interface Person {
  name: string;
  age: number;
  address: Address;
}

---------------- User

/** Test struct with optional fields */
export interface User {
  id: number;
  email: string;
  profile?: string | null;
  verified: boolean;
  tags: { [key: string]: string };
  metadata: { [key: string]: boolean };
}

---------------- Point

/** Test tuple struct */
export type Point = [number, number];

---------------- Unit

/** Test unit struct */
export type Unit = null;

---------------- WrappedString

/**
 * This is a wrapper for a String type. The type
 * should be transparent in the generated code and point directly to the string type.
 */
export type WrappedString = string;

---------------- Animal

/** Simple enum with unit variants */
export type Animal = "Cat" | "Dog" | "Fish";

---------------- Shape

/** Complex enum with different variant types */
export type Shape = 
  /** Circle with radius */
  { "Circle": number } | 
  /** Rectangle with width and height */
  { "Rectangle": [number, number] } | 
  /** Point with coordinates */
  { "Point": { x: number, y: number } };
"#
        );
    }

    #[test]
    fn test_flow_generation() {
        let mut flow_output = String::new();
        for decl in get_all_declarations() {
            flow_output.push_str(&format!("---------------- {}\n\n", &decl.type_name));
            flow_output.push_str(&decl.export_flow());
            flow_output.push_str("\n\n");
        }

        k9::snapshot!(
            flow_output.trim(),
            r#"
---------------- Address

// Simple address struct for testing
export type Address = {
  // Street address
  street: string,
  city: string,
  zip_code: number,
};

---------------- Person

import type { Address } from './Address.js';

// Person struct that references Address
export type Person = {
  name: string,
  age: number,
  address: Address,
};

---------------- User

// Test struct with optional fields
export type User = {
  id: number,
  email: string,
  profile?: ?string,
  verified: boolean,
  tags: { [key: string]: string },
  metadata: { [key: string]: boolean },
};

---------------- Point

// Test tuple struct
export type Point = [number, number];

---------------- Unit

// Test unit struct
export type Unit = null;

---------------- WrappedString

// This is a wrapper for a String type. The type
// should be transparent in the generated code and point directly to the string type.
export type WrappedString = string;

---------------- Animal

// Simple enum with unit variants
export type Animal = "Cat" | "Dog" | "Fish";

---------------- Shape

// Complex enum with different variant types
export type Shape = 
  // Circle with radius
  { "Circle": number } | 
  // Rectangle with width and height
  { "Rectangle": [number, number] } | 
  // Point with coordinates
  { "Point": { x: number, y: number } };
"#
        );
    }

    #[test]
    fn test_type_declarations() {
        k9::snapshot!(
            get_all_declarations(),
            r#"
[
    TypeGenGeneratedType {
        type_name: "Address",
        docs: Some(
            "Simple address struct for testing",
        ),
        file_path: <SANITIZED>/lib.rs,
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
                ],
            },
        ),
    },
    TypeGenGeneratedType {
        type_name: "Person",
        docs: Some(
            "Person struct that references Address",
        ),
        file_path: <SANITIZED>/lib.rs,
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
    },
    TypeGenGeneratedType {
        type_name: "User",
        docs: Some(
            "Test struct with optional fields",
        ),
        file_path: <SANITIZED>/lib.rs,
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
    },
    TypeGenGeneratedType {
        type_name: "Point",
        docs: Some(
            "Test tuple struct",
        ),
        file_path: <SANITIZED>/lib.rs,
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
    },
    TypeGenGeneratedType {
        type_name: "Unit",
        docs: Some(
            "Test unit struct",
        ),
        file_path: <SANITIZED>/lib.rs,
        declaration: Null,
    },
    TypeGenGeneratedType {
        type_name: "WrappedString",
        docs: Some(
            "This is a wrapper for a String type. The type
should be transparent in the generated code and point directly to the string type.",
        ),
        file_path: <SANITIZED>/lib.rs,
        declaration: TupleStructDecl(
            TupleStructDecl {
                fields: [
                    Primitive(
                        String,
                    ),
                ],
            },
        ),
    },
    TypeGenGeneratedType {
        type_name: "Animal",
        docs: Some(
            "Simple enum with unit variants",
        ),
        file_path: <SANITIZED>/lib.rs,
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
                            "A dog  ",
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
    },
    TypeGenGeneratedType {
        type_name: "Shape",
        docs: Some(
            "Complex enum with different variant types",
        ),
        file_path: <SANITIZED>/lib.rs,
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
                        ],
                    },
                ],
            },
        ),
    },
]
"#
        );
    }

    #[test]
    fn test_enum_implementations() {
        // Test that the enum implementations are working
        let animal_decl = Animal::to_type_decl();
        let shape_decl = Shape::to_type_decl();

        assert_eq!(animal_decl.type_name, "Animal");
        assert_eq!(shape_decl.type_name, "Shape");

        // Test TypeScript generation
        let animal_ts = animal_decl.export_typescript();
        let shape_ts = shape_decl.export_typescript();

        assert!(animal_ts.contains("Animal"));
        assert!(animal_ts.contains("Cat"));
        assert!(shape_ts.contains("Shape"));
        assert!(shape_ts.contains("Circle"));
    }
}
