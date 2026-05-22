// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::Lang;
use crate::TypeGenConfig;
use crate::docs::DocFormat;
use crate::docs::render_docs;
use crate::types::EnumDecl;
use crate::types::EnumVariant;
use crate::types::PrimitiveTypeRef;
use crate::types::StructDecl;
use crate::types::TupleStructDecl;
use crate::types::TypeGenDecl;
use crate::types::TypeGenGeneratedType;
use crate::types::TypeRef;

/// Convert a CamelCase name to SCREAMING_SNAKE_CASE.
///
/// Examples:
/// - `"Cat"` → `"CAT"`
/// - `"HelloWorld"` → `"HELLO_WORLD"`
/// - `"XMLParser"` → `"XML_PARSER"`
/// - `"getHTTPResponse"` → `"GET_HTTP_RESPONSE"`
fn to_screaming_snake_case(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let prev = chars[i - 1];
            // Insert underscore before an uppercase letter when:
            // 1. Previous char is lowercase (e.g., "helloW" → "hello_W")
            // 2. Previous char is uppercase but next char is lowercase (e.g., "XMLParser" → "XML_Parser")
            if prev.is_lowercase()
                || (prev.is_uppercase() && i + 1 < chars.len() && chars[i + 1].is_lowercase())
            {
                result.push('_');
            }
        }
        result.push(ch.to_ascii_uppercase());
    }

    result
}

/// Hack type code generator
pub struct HackGenerator<'a> {
    pub config: &'a TypeGenConfig,
    pub generated_type: &'a TypeGenGeneratedType,
}

impl<'a> HackGenerator<'a> {
    /// Generate Hack code from a type declaration
    pub fn generate(config: &'a TypeGenConfig, generated_type: &'a TypeGenGeneratedType) -> String {
        Self {
            config,
            generated_type,
        }
        .generate_impl()
    }

    fn generate_impl(&self) -> String {
        // Check if there's a Hack override for this type
        if let Some(ref overrides) = self.generated_type.overrides {
            if let Some(ref hack_override) = overrides.hack {
                let type_name = self
                    .config
                    .get_type_name(&self.generated_type.original_type_name, Lang::Hack);

                // Add docs if available
                let docs = render_docs(&self.generated_type.docs, DocFormat::TwoSlash, 0);

                return format!("{}type {} = {};\n", docs, type_name, hack_override);
            }
        }

        let type_name = self
            .config
            .get_type_name(&self.generated_type.original_type_name, Lang::Hack);

        let type_code = match &self.generated_type.declaration {
            TypeGenDecl::StructDecl(struct_decl) => {
                self.generate_struct_hack(&type_name, struct_decl)
            }
            TypeGenDecl::TupleStructDecl(tuple_struct_decl) => {
                self.generate_tuple_struct_hack(&type_name, tuple_struct_decl)
            }
            TypeGenDecl::EnumDecl(enum_decl) => self.generate_enum_hack(&type_name, enum_decl),
            TypeGenDecl::Null => self.generate_null_hack(&type_name),
        }; // Generate use statements
        let mut result = String::new();

        result.push_str(&type_code);
        result
    }

    fn generate_struct_hack(&self, type_name: &str, struct_decl: &StructDecl) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(
            &self.generated_type.docs,
            DocFormat::TwoSlash,
            0,
        ));

        result.push_str(&format!("type {} = shape(\n", type_name));

        for field in &struct_decl.fields {
            let field_type = self.type_ref_to_hack(&field.type_ref);

            result.push_str(&render_docs(&field.docs, DocFormat::TwoSlash, 2));

            let question_mark = matches!(field.type_ref, TypeRef::Option(_))
                .then(|| "?")
                .unwrap_or_default();

            result.push_str(&format!(
                "  {}'{}' => {},\n",
                question_mark, field.field_name, field_type
            ));
        }

        result.push_str(");\n");
        result
    }

    fn generate_tuple_struct_hack(
        &self,
        type_name: &str,
        tuple_struct_decl: &TupleStructDecl,
    ) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(
            &self.generated_type.docs,
            DocFormat::TwoSlash,
            0,
        ));

        let field_types: Vec<String> = tuple_struct_decl
            .fields
            .iter()
            .map(|f| self.type_ref_to_hack(f))
            .collect();

        // For single-element tuples, just use the inner type directly
        if field_types.len() == 1 {
            result.push_str(&format!("type {} = {};\n", type_name, field_types[0]));
        } else {
            result.push_str(&format!(
                "type {type_name} = ({});\n",
                field_types.join(", ")
            ));
        }
        result
    }

    fn generate_enum_hack(&self, type_name: &str, enum_decl: &EnumDecl) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(
            &self.generated_type.docs,
            DocFormat::TwoSlash,
            0,
        ));

        // Check if this is a simple enum (all unit variants)
        let is_simple_enum = enum_decl
            .variants
            .iter()
            .all(|variant| matches!(variant, EnumVariant::Unit { .. }));

        let has_any_unit = enum_decl
            .variants
            .iter()
            .any(|variant| matches!(variant, EnumVariant::Unit { .. }));

        // Mixed enums (some unit, some data) are not supported — they cause
        // serialization mismatches between serde and typegen-generated Hack shapes.
        if !is_simple_enum && has_any_unit {
            panic!(
                "TypeGen: enum '{}' has mixed variants (some unit, some data). \
                 Either all variants must be unit variants or all must carry data.",
                type_name
            );
        }

        if is_simple_enum {
            // Generate Hack enum for simple enums
            result.push_str(&format!("enum {}: string as string {{\n", type_name));
            for variant in &enum_decl.variants {
                if let EnumVariant::Unit { name, docs } = variant {
                    let constant_name = to_screaming_snake_case(name);
                    result.push_str(&render_docs(docs, DocFormat::TwoSlash, 2));
                    result.push_str(&format!("  {} = \"{}\";\n", constant_name, name));
                }
            }
            result.push_str("}\n");
        } else {
            // For complex enums, use a shape with all variants as keys
            result.push_str(&format!("type {} = shape(\n", type_name));

            for variant in &enum_decl.variants {
                match variant {
                    EnumVariant::Unit { name, docs } => {
                        result.push_str(&render_docs(docs, DocFormat::TwoSlash, 2));
                        // Unit variants get null as value
                        result.push_str(&format!("  ?'{}' => ?null,\n", name));
                    }
                    EnumVariant::Newtype {
                        name,
                        field_type,
                        docs,
                    } => {
                        result.push_str(&render_docs(docs, DocFormat::TwoSlash, 2));
                        // Newtype variants get the wrapped type
                        let hack_type = self.type_ref_to_hack(field_type);
                        result.push_str(&format!("  ?'{}' => ?{},\n", name, hack_type));
                    }
                    EnumVariant::Tuple { name, fields, docs } => {
                        result.push_str(&render_docs(docs, DocFormat::TwoSlash, 2));

                        // Tuple variants get a tuple type
                        let field_types: Vec<String> =
                            fields.iter().map(|f| self.type_ref_to_hack(f)).collect();

                        if field_types.len() == 1 {
                            result.push_str(&format!("  ?'{}' => ?{},\n", name, field_types[0]));
                        } else {
                            result.push_str(&format!(
                                "  ?'{}' => ?({}),\n",
                                name,
                                field_types.join(", ")
                            ));
                        }
                    }
                    EnumVariant::Struct { name, fields, docs } => {
                        result.push_str(&render_docs(docs, DocFormat::TwoSlash, 2));
                        // Struct variants get a shape with their fields
                        if fields.is_empty() {
                            result.push_str(&format!("  ?'{}' => ?shape(),\n", name));
                        } else {
                            result.push_str(&format!("  ?'{}' => ?shape(\n", name));
                            for field in fields {
                                let field_type = self.type_ref_to_hack(&field.type_ref);
                                result.push_str(&format!(
                                    "    '{}' => {},\n",
                                    field.field_name, field_type
                                ));
                            }
                            result.push_str("  ),\n");
                        }
                    }
                }
            }

            result.push_str(");\n\n");

            // For complex enums, also generate an enum with all variant names
            result.push_str(&format!("enum {}Variant: string as string {{\n", type_name));
            for variant in &enum_decl.variants {
                let variant_name = match variant {
                    EnumVariant::Unit { name, .. } => name,
                    EnumVariant::Newtype { name, .. } => name,
                    EnumVariant::Tuple { name, .. } => name,
                    EnumVariant::Struct { name, .. } => name,
                };
                let constant_name = to_screaming_snake_case(variant_name);
                result.push_str(&format!("  {} = \"{}\";\n", constant_name, variant_name));
            }
            result.push_str("}\n");
        }
        result
    }

    fn generate_null_hack(&self, type_name: &str) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(
            &self.generated_type.docs,
            DocFormat::TwoSlash,
            0,
        ));

        result.push_str(&format!("type {} = null;\n", type_name));
        result
    }

    fn type_ref_to_hack(&self, type_ref: &TypeRef) -> String {
        match type_ref {
            TypeRef::Primitive(primitive) => Self::primitive_to_hack(primitive),
            TypeRef::Option(inner) => {
                format!("?{}", self.type_ref_to_hack(inner))
            }
            TypeRef::Vec(inner) => {
                format!("vec<{}>", self.type_ref_to_hack(inner))
            }
            TypeRef::Set(inner) => {
                format!("keyset<{}>", self.type_ref_to_hack(inner))
            }
            TypeRef::Array {
                element_type,
                size: _,
            } => {
                // Hack doesn't have fixed-size arrays, so we'll use vec
                format!("vec<{}>", self.type_ref_to_hack(element_type))
            }
            TypeRef::Map { key, value } => {
                format!(
                    "dict<{}, {}>",
                    self.type_ref_to_hack(key),
                    self.type_ref_to_hack(value)
                )
            }
            TypeRef::TypeReference(type_name) => self.config.get_type_name(type_name, Lang::Hack),
        }
    }

    fn primitive_to_hack(primitive: &PrimitiveTypeRef) -> String {
        match primitive {
            PrimitiveTypeRef::String => "string".to_string(),
            PrimitiveTypeRef::Bool => "bool".to_string(),
            PrimitiveTypeRef::I8
            | PrimitiveTypeRef::I16
            | PrimitiveTypeRef::I32
            | PrimitiveTypeRef::I64
            | PrimitiveTypeRef::I128
            | PrimitiveTypeRef::ISize
            | PrimitiveTypeRef::U8
            | PrimitiveTypeRef::U16
            | PrimitiveTypeRef::U32
            | PrimitiveTypeRef::U64
            | PrimitiveTypeRef::U128
            | PrimitiveTypeRef::USize => "int".to_string(),
            PrimitiveTypeRef::F32 | PrimitiveTypeRef::F64 => "float".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::to_screaming_snake_case;

    #[test]
    fn test_to_screaming_snake_case() {
        // Single words
        assert_eq!(to_screaming_snake_case("Cat"), "CAT");
        assert_eq!(to_screaming_snake_case("Hello"), "HELLO");

        // Multi-word CamelCase
        assert_eq!(to_screaming_snake_case("HelloWorld"), "HELLO_WORLD");
        assert_eq!(to_screaming_snake_case("GetRequest"), "GET_REQUEST");
        assert_eq!(to_screaming_snake_case("PostRequest"), "POST_REQUEST");
        assert_eq!(to_screaming_snake_case("DeleteAll"), "DELETE_ALL");

        // Acronyms
        assert_eq!(to_screaming_snake_case("XMLParser"), "XML_PARSER");
        assert_eq!(
            to_screaming_snake_case("getHTTPResponse"),
            "GET_HTTP_RESPONSE"
        );
        assert_eq!(to_screaming_snake_case("SimpleA"), "SIMPLE_A");

        // Already uppercase single char
        assert_eq!(to_screaming_snake_case("A"), "A");

        // All uppercase
        assert_eq!(to_screaming_snake_case("URL"), "URL");
    }
}
