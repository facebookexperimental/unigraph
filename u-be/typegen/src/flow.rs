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

/// Flow.js code generator
pub struct FlowGenerator;

impl FlowGenerator {
    /// Generate Flow.js code from a type declaration
    pub fn generate_flow(config: &TypeGenConfig, generated_type: &TypeGenGeneratedType) -> String {
        // Check if this type should be skipped for Flow
        if let Some(ref skip) = generated_type.skip {
            if skip.flow {
                return String::new(); // Return empty string to skip generation
            }
        }

        // Check if there's a Flow override for this type
        if let Some(ref overrides) = generated_type.overrides {
            if let Some(ref flow_override) = overrides.flow {
                let type_name =
                    config.get_type_name(&generated_type.original_type_name, Lang::Flow);

                // Add docs if available
                let docs = render_docs(&generated_type.docs, DocFormat::TwoSlash, 0);

                return format!("{}export type {} = {};\n", docs, type_name, flow_override);
            }
        }
        let type_name = config.get_type_name(&generated_type.original_type_name, Lang::Flow);

        let mut imports = std::collections::HashSet::new();
        let type_code = match &generated_type.declaration {
            TypeGenDecl::StructDecl(struct_decl) => Self::generate_struct_flow(
                &type_name,
                &generated_type.docs,
                struct_decl,
                &mut imports,
            ),
            TypeGenDecl::TupleStructDecl(tuple_struct_decl) => Self::generate_tuple_struct_flow(
                &type_name,
                &generated_type.docs,
                tuple_struct_decl,
                &mut imports,
            ),
            TypeGenDecl::EnumDecl(enum_decl) => {
                Self::generate_enum_flow(&type_name, &generated_type.docs, enum_decl, &mut imports)
            }
            TypeGenDecl::Null => Self::generate_null_flow(&type_name, &generated_type.docs),
        };

        // Generate import statements
        let mut result = String::new();
        if !imports.is_empty() {
            let mut sorted_imports: Vec<_> = imports.into_iter().collect();
            sorted_imports.sort();
            for import_original_type_name in sorted_imports {
                result.push_str(&format!(
                    "import type {{ {} }} from './{}';\n",
                    config.get_type_name(&import_original_type_name, Lang::Flow),
                    config
                        .make_file_name(&import_original_type_name, Lang::Flow)
                        .display()
                ));
            }
            result.push('\n');
        }

        result.push_str(&type_code);
        result
    }

    fn generate_struct_flow(
        type_name: &str,
        docs: &Option<String>,
        struct_decl: &StructDecl,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(docs, DocFormat::Block, 0));
        // Named struct - use object type
        result.push_str(&format!("export type {} = {{\n", type_name));

        for field in &struct_decl.fields {
            result.push_str(&render_docs(&field.docs, DocFormat::Block, 2));
            let field_name = &field.field_name;
            let question_mark = matches!(field.type_ref, TypeRef::Option(_))
                .then(|| "?")
                .unwrap_or_default();

            let flow_type = Self::resolve_flow_type(&field.type_ref, imports);

            result.push_str(&format!(
                "  {}{}: {},\n",
                field_name, question_mark, flow_type
            ));
        }

        result.push_str("};");

        result
    }

    fn generate_tuple_struct_flow(
        type_name: &str,
        docs: &Option<String>,
        tuple_struct_decl: &TupleStructDecl,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(docs, DocFormat::Block, 0));

        // For single-field tuple structs, make them transparent (direct type alias)
        if tuple_struct_decl.fields.len() == 1 {
            let inner_type = Self::resolve_flow_type(&tuple_struct_decl.fields[0], imports);
            result.push_str(&format!("export type {} = {};", type_name, inner_type));
        } else {
            // Multi-field tuple struct - generate as tuple type
            let types: Vec<String> = tuple_struct_decl
                .fields
                .iter()
                .map(|type_ref| Self::resolve_flow_type(type_ref, imports))
                .collect();

            result.push_str(&format!(
                "export type {} = [{}];",
                type_name,
                types.join(", ")
            ));
        }

        result
    }

    fn generate_enum_flow(
        type_name: &str,
        docs: &Option<String>,
        enum_decl: &EnumDecl,
        imports: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(docs, DocFormat::Block, 0));

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
                        variant_result.push_str(&render_docs(docs, DocFormat::Block, 2));
                        variant_result.push_str(&format!("  \"{}\"", name));
                        variant_result
                    }
                    EnumVariant::Newtype {
                        name,
                        docs,
                        field_type,
                    } => {
                        let mut variant_result = String::new();
                        variant_result.push_str(&render_docs(docs, DocFormat::Block, 2));
                        let flow_type = Self::resolve_flow_type(field_type, imports);
                        variant_result.push_str(&format!("  {{ \"{}\": {} }}", name, flow_type));
                        variant_result
                    }
                    EnumVariant::Tuple { name, docs, fields } => {
                        let mut variant_result = String::new();
                        variant_result.push_str(&render_docs(docs, DocFormat::Block, 2));
                        let field_types: Vec<String> = fields
                            .iter()
                            .map(|field_type| Self::resolve_flow_type(field_type, imports))
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
                        variant_result.push_str(&render_docs(docs, DocFormat::Block, 2));
                        let struct_fields: Vec<String> = fields
                            .iter()
                            .map(|field| {
                                let field_name = &field.field_name;
                                let flow_type = Self::resolve_flow_type(&field.type_ref, imports);
                                format!("{}: {}", field_name, flow_type)
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

    /// Resolve a type reference to its Flow equivalent
    fn resolve_flow_type(
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
                format!("?{}", Self::resolve_flow_type(inner, imports))
            }
            TypeRef::Vec(inner) => {
                format!("Array<{}>", Self::resolve_flow_type(inner, imports))
            }
            TypeRef::Array { element_type, size } => {
                let element_flow = Self::resolve_flow_type(element_type, imports);
                // Generate tuple type like [number, number, number]
                let elements = (0..*size).map(|_| element_flow.clone()).collect::<Vec<_>>();
                format!("[{}]", elements.join(", "))
            }
            TypeRef::Map { key, value } => {
                format!(
                    "{{ [key: {}]: {} }}",
                    Self::resolve_flow_type(key, imports),
                    Self::resolve_flow_type(value, imports)
                )
            }
        }
    }

    fn generate_null_flow(type_name: &str, docs: &Option<String>) -> String {
        let mut result = String::new();

        result.push_str(&render_docs(docs, DocFormat::Block, 0));

        // Null type (for unit structs)
        result.push_str(&format!("export type {} = null;", type_name));

        result
    }
}
