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
        };

        // Generate use statements
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

            result.push_str(&format!("  '{}' => {},\n", field.field_name, field_type));
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

        if is_simple_enum {
            // Generate Hack enum for simple enums
            result.push_str(&format!("enum {}: string as string {{\n", type_name));
            for variant in &enum_decl.variants {
                if let EnumVariant::Unit { name, docs } = variant {
                    let constant_name = name.to_uppercase();
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

            result.push_str(");\n");
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
