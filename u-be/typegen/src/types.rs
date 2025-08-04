use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

/// Enum representing different type references
#[derive(Debug, Clone)]
pub enum TypeRef {
    TypeReference(String),
    Primitive(PrimitiveTypeRef),
    Option(Box<TypeRef>),
    Vec(Box<TypeRef>),
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
}

/// Enum for primitive/built-in types
#[derive(Debug, Clone)]
pub enum PrimitiveTypeRef {
    String,
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    F32,
    F64,
}

#[derive(Clone)]
pub struct SourceFilePath(pub PathBuf);

impl AsRef<Path> for SourceFilePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Because we get different paths in different environments, we sanitize the output
/// to only show the last segment of the path, which is usually the file name.
/// This way we can write snapshot tests.
impl std::fmt::Debug for SourceFilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // let get last segment of the path
        if let Some(last) = self.0.file_name() {
            write!(f, "<SANITIZED>/{}", last.to_string_lossy())
        } else {
            write!(f, "<SANITIZED>")
        }
    }
}

/// Top-level struct representing a generated type with shared metadata
#[derive(Debug, Clone)]
pub struct TypeGenGeneratedType {
    pub type_name: String,
    pub docs: Option<String>,
    pub file_path: SourceFilePath,
    pub declaration: TypeGenDecl,
}

impl TypeGenGeneratedType {
    /// Export TypeScript type definition
    pub fn export_typescript(&self) -> String {
        crate::typescript::TypeScriptGenerator::generate_typescript(self)
    }

    /// Export Flow type definition
    pub fn export_flow(&self) -> String {
        crate::flow::FlowGenerator::generate_flow(self)
    }

    /// Write this type to TypeScript and Flow files based on the configuration
    /// found by looking up the directory tree from the source file path.
    /// Only writes files when TYPEGEN=1 environment variable is set.
    pub fn write_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Only proceed if TYPEGEN=1 environment variable is set
        let enabled = std::env::var("TYPEGEN").unwrap_or_default() == "1";

        if !enabled {
            return Ok(());
        }

        // Get the config for this file's directory
        let config = crate::config::get_config_for_file(&self.file_path);

        // Generate TypeScript if configured
        if let Some(ref ts_export_path) = config.typescript.export_path {
            let ts_content = crate::typescript::TypeScriptGenerator::generate_typescript(self);
            let ts_file_path = format!("{}/{}.ts", ts_export_path, self.type_name);
            crate::config::write_type_to_file(&ts_content, &ts_file_path)?;
            println!("Wrote TypeScript definition to: {}", ts_file_path);
        }

        // Generate Flow if configured
        if let Some(ref flow_export_path) = config.flow.export_path {
            let flow_content = crate::flow::FlowGenerator::generate_flow(self);
            let flow_file_path = format!("{}/{}.js", flow_export_path, self.type_name);
            crate::config::write_type_to_file(&flow_content, &flow_file_path)?;
            println!("Wrote Flow definition to: {}", flow_file_path);
        }

        Ok(())
    }
}

/// Abstract representation of a type declaration (without shared fields)
#[derive(Debug, Clone)]
pub enum TypeGenDecl {
    StructDecl(StructDecl),
    TupleStructDecl(TupleStructDecl),
    EnumDecl(EnumDecl),
    Null,
}

/// Abstract representation of a struct declaration
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub fields: Vec<FieldDeclaration>,
}

/// Abstract representation of a tuple struct declaration
#[derive(Debug, Clone)]
pub struct TupleStructDecl {
    pub fields: Vec<TypeRef>,
}

/// Abstract representation of an enum declaration
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub variants: Vec<EnumVariant>,
}

/// Abstract representation of a field in a struct
#[derive(Debug, Clone)]
pub struct FieldDeclaration {
    pub field_name: String,
    pub type_ref: TypeRef,
    pub docs: Option<String>,
}

/// Abstract representation of an enum variant
#[derive(Debug, Clone)]
pub enum EnumVariant {
    /// Unit variant: Variant
    Unit { name: String, docs: Option<String> },
    /// Newtype variant: Variant(Type)
    Newtype {
        name: String,
        docs: Option<String>,
        field_type: TypeRef,
    },
    /// Tuple variant: Variant(Type1, Type2, ...)
    Tuple {
        name: String,
        docs: Option<String>,
        fields: Vec<TypeRef>,
    },
    /// Struct variant: Variant { field1: Type1, field2: Type2, ... }
    Struct {
        name: String,
        docs: Option<String>,
        fields: Vec<FieldDeclaration>,
    },
}

/// Trait for types that can provide a type reference
pub trait TypeGenTypeRefTrait {
    /// Returns the type reference
    fn type_ref() -> TypeRef;
}

/// Trait for types that can provide their abstract type declaration
pub trait TypeGenDeclTrait: TypeGenTypeRefTrait {
    /// Returns the abstract type declaration structure
    fn to_type_decl() -> TypeGenGeneratedType;
}

// Implement TypeGenTypeRefTrait for primitive types
impl TypeGenTypeRefTrait for String {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::String)
    }
}

impl TypeGenTypeRefTrait for str {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::String)
    }
}

impl TypeGenTypeRefTrait for bool {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::Bool)
    }
}

impl TypeGenTypeRefTrait for i8 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::I8)
    }
}

impl TypeGenTypeRefTrait for i16 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::I16)
    }
}

impl TypeGenTypeRefTrait for i32 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::I32)
    }
}

impl TypeGenTypeRefTrait for i64 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::I64)
    }
}

impl TypeGenTypeRefTrait for i128 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::I128)
    }
}

impl TypeGenTypeRefTrait for isize {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::ISize)
    }
}

impl TypeGenTypeRefTrait for u8 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::U8)
    }
}

impl TypeGenTypeRefTrait for u16 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::U16)
    }
}

impl TypeGenTypeRefTrait for u32 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::U32)
    }
}

impl TypeGenTypeRefTrait for u64 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::U64)
    }
}

impl TypeGenTypeRefTrait for u128 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::U128)
    }
}

impl TypeGenTypeRefTrait for usize {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::USize)
    }
}

impl TypeGenTypeRefTrait for f32 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::F32)
    }
}

impl TypeGenTypeRefTrait for f64 {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::F64)
    }
}

impl TypeGenTypeRefTrait for char {
    fn type_ref() -> TypeRef {
        TypeRef::Primitive(PrimitiveTypeRef::String) // Treat char as string for TS/Flow
    }
}

// Implement for common collections
impl<T: TypeGenTypeRefTrait> TypeGenTypeRefTrait for Vec<T> {
    fn type_ref() -> TypeRef {
        TypeRef::Vec(Box::new(T::type_ref()))
    }
}

impl<T: TypeGenTypeRefTrait> TypeGenTypeRefTrait for Option<T> {
    fn type_ref() -> TypeRef {
        TypeRef::Option(Box::new(T::type_ref()))
    }
}

impl<K: TypeGenTypeRefTrait, V: TypeGenTypeRefTrait> TypeGenTypeRefTrait for HashMap<K, V> {
    fn type_ref() -> TypeRef {
        // For now, represent as a type reference - this could be improved
        TypeRef::Map {
            key: Box::new(K::type_ref()),
            value: Box::new(V::type_ref()),
        }
    }
}

impl<K: TypeGenTypeRefTrait, V: TypeGenTypeRefTrait> TypeGenTypeRefTrait for BTreeMap<K, V> {
    fn type_ref() -> TypeRef {
        // For now, represent as a type reference - this could be improved
        TypeRef::Map {
            key: Box::new(K::type_ref()),
            value: Box::new(V::type_ref()),
        }
    }
}
