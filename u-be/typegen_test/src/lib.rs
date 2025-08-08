use std::collections::BTreeMap;
use std::collections::HashMap;

#[cfg(test)]
mod shared;
#[cfg(test)]
mod test_flow;
#[cfg(test)]
mod test_hack;
#[cfg(test)]
mod test_ts;

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

/// Test struct with type overrides
#[derive(TypeGen)]
#[typegen(Hack("null"), TypeScript("() => void"), Flow("() => void"))]
pub struct OverrideTest;

/// Test struct that skips generation for Hack and Flow
#[derive(TypeGen)]
#[typegen(skip(Hack, Flow))]
pub struct SkipTest {
    pub value: u32,
}

/// Test struct that combines skips and overrides
#[derive(TypeGen)]
#[typegen(skip(Flow), TypeScript("string"))]
pub struct SkipAndOverrideTest {
    pub data: u32,
}

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
    use crate::shared::get_all_declarations;

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
    original_type_name: "Address",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "Person",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "User",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "Point",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "Unit",
    docs: Some(
        "Test unit struct",
    ),
    file_path: <SANITIZED>
    declaration: Null,
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "WrappedString",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "Animal",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "Shape",
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
    overrides: None,
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "OverrideTest",
    docs: Some(
        "Test struct with type overrides",
    ),
    file_path: <SANITIZED>
    declaration: Null,
    overrides: Some(
        TypeGenOverrides {
            hack: Some(
                "null",
            ),
            flow: Some(
                "() => void",
            ),
            typescript: Some(
                "() => void",
            ),
        },
    ),
    skip: None,
}

TypeGenGeneratedType {
    original_type_name: "SkipTest",
    docs: Some(
        "Test struct that skips generation for Hack and Flow",
    ),
    file_path: <SANITIZED>
    declaration: StructDecl(
        StructDecl {
            fields: [
                FieldDeclaration {
                    field_name: "value",
                    type_ref: Primitive(
                        U32,
                    ),
                    docs: None,
                },
            ],
        },
    ),
    overrides: None,
    skip: Some(
        TypeGenSkip {
            hack: true,
            flow: true,
            typescript: false,
        },
    ),
}

TypeGenGeneratedType {
    original_type_name: "SkipAndOverrideTest",
    docs: Some(
        "Test struct that combines skips and overrides",
    ),
    file_path: <SANITIZED>
    declaration: StructDecl(
        StructDecl {
            fields: [
                FieldDeclaration {
                    field_name: "data",
                    type_ref: Primitive(
                        U32,
                    ),
                    docs: None,
                },
            ],
        },
    ),
    overrides: Some(
        TypeGenOverrides {
            hack: None,
            flow: None,
            typescript: Some(
                "string",
            ),
        },
    ),
    skip: Some(
        TypeGenSkip {
            hack: false,
            flow: true,
            typescript: false,
        },
    ),
}
"#
        );
    }
}
