use crate::Lang;
use crate::TypeGenConfig;
use crate::types::EnumDecl;
use crate::types::EnumVariant;
use crate::types::PrimitiveTypeRef;
use crate::types::StructDecl;
use crate::types::TupleStructDecl;
use crate::types::TypeGenDecl;
use crate::types::TypeGenGeneratedType;
use crate::types::TypeRef;

/// TypeScript code generator
pub struct TypeScriptGenerator;

impl TypeScriptGenerator {
    /// Generate TypeScript code from a type declaration
    pub fn generate_typescript(
        config: &TypeGenConfig,
        generated_type: &TypeGenGeneratedType,
    ) -> String {
        let mut imports = std::collections::HashSet::new();
        let type_name = config.get_type_name(&generated_type.original_type_name, Lang::TypeScript);

        let type_code = match &generated_type.declaration {
            TypeGenDecl::StructDecl(struct_decl) => Self::generate_struct_typescript(
                &type_name,
                &generated_type.docs,
                struct_decl,
                &mut imports,
            ),
            TypeGenDecl::TupleStructDecl(tuple_struct_decl) => {
                Self::generate_tuple_struct_typescript(
                    &type_name,
                    &generated_type.docs,
                    tuple_struct_decl,
                    &mut imports,
                )
            }
            TypeGenDecl::EnumDecl(enum_decl) => Self::generate_enum_typescript(
                &type_name,
                &generated_type.docs,
                enum_decl,
                &mut imports,
            ),
            TypeGenDecl::Null => Self::generate_null_typescript(&type_name, &generated_type.docs),
        };

        // Generate import statements
        let mut result = String::new();
        if !imports.is_empty() {
            let mut sorted_imports: Vec<_> = imports.into_iter().collect();
            sorted_imports.sort();
            for import_original_type_name in sorted_imports {
                result.push_str(&format!(
                    "import type {{ {} }} from './{}';\n",
                    config.get_type_name(&import_original_type_name, Lang::TypeScript),
                    config
                        .typescript_file_name(&import_original_type_name)
                        .display()
                ));
            }
            result.push('\n');
        }

        result.push_str(&type_code);
        result
    }

    fn generate_struct_typescript(
        type_name: &str,
        docs: &Option<String>,
        struct_decl: &StructDecl,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut result = String::new();

        // Add documentation if present
        if let Some(docs) = docs {
            result.push_str(&Self::format_ts_jsdoc(docs));
        }

        // Named struct - use interface
        result.push_str(&format!("export interface {} {{\n", type_name));

        for field in &struct_decl.fields {
            if let Some(field_docs) = &field.docs {
                result.push_str(&format!("  /** {} */\n", field_docs.replace('\n', " ")));
            }

            let field_name = &field.field_name;
            let question_mark = matches!(field.type_ref, TypeRef::Option(_))
                .then(|| "?")
                .unwrap_or_default();
            let ts_type = Self::resolve_typescript_type(&field.type_ref, imports);

            result.push_str(&format!(
                "  {}{}: {};\n",
                field_name, question_mark, ts_type
            ));
        }

        result.push('}');

        result
    }

    fn generate_tuple_struct_typescript(
        type_name: &str,
        docs: &Option<String>,
        tuple_struct_decl: &TupleStructDecl,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut result = String::new();

        // Add documentation if present
        if let Some(docs) = docs {
            result.push_str(&Self::format_ts_jsdoc(docs));
        }

        // For single-field tuple structs, make them transparent (direct type alias)
        if tuple_struct_decl.fields.len() == 1 {
            let inner_type = Self::resolve_typescript_type(&tuple_struct_decl.fields[0], imports);
            result.push_str(&format!("export type {} = {};", type_name, inner_type));
        } else {
            // Multi-field tuple struct - generate as tuple type
            let types: Vec<String> = tuple_struct_decl
                .fields
                .iter()
                .map(|type_ref| Self::resolve_typescript_type(type_ref, imports))
                .collect();

            result.push_str(&format!(
                "export type {} = [{}];",
                type_name,
                types.join(", ")
            ));
        }

        result
    }

    fn generate_enum_typescript(
        type_name: &str,
        docs: &Option<String>,
        enum_decl: &EnumDecl,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut result = String::new();

        // Add documentation if present
        if let Some(docs) = docs {
            result.push_str(&Self::format_ts_jsdoc(docs));
        }

        // Check if this is a simple enum (all unit variants)
        let is_simple_enum = enum_decl
            .variants
            .iter()
            .all(|variant| matches!(variant, EnumVariant::Unit { .. }));

        if is_simple_enum {
            // Simple enum - generate union type
            let variants: Vec<String> = enum_decl
                .variants
                .iter()
                .map(|variant| match variant {
                    EnumVariant::Unit { name, .. } => format!("\"{}\"", name),
                    _ => unreachable!(), // We already checked all are unit variants
                })
                .collect();

            result.push_str(&format!(
                "export type {} = {};",
                type_name,
                variants.join(" | ")
            ));
        } else {
            // Complex enum - use externally tagged representation (serde default)
            let variants: Vec<String> = enum_decl
                .variants
                .iter()
                .map(|variant| match variant {
                    EnumVariant::Unit { name, docs } => {
                        let mut variant_result = String::new();
                        if let Some(docs) = docs {
                            variant_result
                                .push_str(&format!("  /** {} */\n", docs.replace('\n', " ")));
                        }
                        variant_result.push_str(&format!("  \"{}\"", name));
                        variant_result
                    }
                    EnumVariant::Newtype {
                        name,
                        docs,
                        field_type,
                    } => {
                        let mut variant_result = String::new();
                        if let Some(docs) = docs {
                            variant_result
                                .push_str(&format!("  /** {} */\n", docs.replace('\n', " ")));
                        }
                        let ts_type = Self::resolve_typescript_type(field_type, imports);
                        variant_result.push_str(&format!("  {{ \"{}\": {} }}", name, ts_type));
                        variant_result
                    }
                    EnumVariant::Tuple { name, docs, fields } => {
                        let mut variant_result = String::new();
                        if let Some(docs) = docs {
                            variant_result
                                .push_str(&format!("  /** {} */\n", docs.replace('\n', " ")));
                        }
                        let field_types: Vec<String> = fields
                            .iter()
                            .map(|field_type| Self::resolve_typescript_type(field_type, imports))
                            .collect();
                        variant_result.push_str(&format!(
                            "  {{ \"{}\": [{}] }}",
                            name,
                            field_types.join(", ")
                        ));
                        variant_result
                    }
                    EnumVariant::Struct { name, docs, fields } => {
                        let mut variant_result = String::new();
                        if let Some(docs) = docs {
                            variant_result
                                .push_str(&format!("  /** {} */\n", docs.replace('\n', " ")));
                        }
                        let struct_fields: Vec<String> = fields
                            .iter()
                            .map(|field| {
                                let field_name = &field.field_name;
                                let ts_type =
                                    Self::resolve_typescript_type(&field.type_ref, imports);
                                format!("{}: {}", field_name, ts_type)
                            })
                            .collect();
                        variant_result.push_str(&format!(
                            "  {{ \"{}\": {{ {} }} }}",
                            name,
                            struct_fields.join(", ")
                        ));
                        variant_result
                    }
                })
                .collect();

            result.push_str(&format!(
                "export type {} =\n{};",
                type_name,
                variants.join(" |\n")
            ));
        }

        result
    }

    /// Resolve a type reference to its TypeScript equivalent
    fn resolve_typescript_type(
        type_ref: &TypeRef,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        match type_ref {
            TypeRef::Primitive(prim) => match prim {
                PrimitiveTypeRef::String => "string".to_string(),
                PrimitiveTypeRef::Bool => "boolean".to_string(),
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
                | PrimitiveTypeRef::USize
                | PrimitiveTypeRef::F32
                | PrimitiveTypeRef::F64 => "number".to_string(),
            },
            TypeRef::TypeReference(name) => {
                // Add to imports if it's not a primitive type
                imports.insert(name.clone());
                name.clone()
            }
            TypeRef::Option(inner) => {
                format!(
                    "{} | undefined",
                    Self::resolve_typescript_type(inner, imports)
                )
            }
            TypeRef::Vec(inner) => {
                format!("{}[]", Self::resolve_typescript_type(inner, imports))
            }
            TypeRef::Array { element_type, size } => {
                let element_ts = Self::resolve_typescript_type(element_type, imports);
                // Generate tuple type like [number, number, number]
                let elements = (0..*size).map(|_| element_ts.clone()).collect::<Vec<_>>();
                format!("[{}]", elements.join(", "))
            }
            TypeRef::Map { key, value } => {
                format!(
                    "{{ [key: {}]: {} }}",
                    Self::resolve_typescript_type(key, imports),
                    Self::resolve_typescript_type(value, imports)
                )
            }
        }
    }

    fn generate_null_typescript(type_name: &str, docs: &Option<String>) -> String {
        let mut result = String::new();

        // Add documentation if present
        if let Some(docs) = docs {
            result.push_str(&Self::format_ts_jsdoc(docs));
        }

        // Null type (for unit structs)
        result.push_str(&format!("export type {} = null;", type_name));

        result
    }

    /// Format documentation as TypeScript JSDoc comment
    fn format_ts_jsdoc(doc: &str) -> String {
        if doc.is_empty() {
            String::new()
        } else {
            let lines: Vec<&str> = doc.lines().collect();
            if lines.len() == 1 {
                format!("/** {} */\n", lines[0])
            } else {
                let formatted_lines = lines
                    .iter()
                    .map(|line| format!(" * {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("/**\n{}\n */\n", formatted_lines)
            }
        }
    }
}
