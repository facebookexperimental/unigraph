// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(clippy::single_match)]
#![allow(clippy::collapsible_if)]

use proc_macro::TokenStream;
use quote::quote;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;

/// Declare a named group of constants mirrored into every configured target
/// language.
///
/// ```ignore
/// typegen_consts! {
///     /// Well-known timeline identifiers.
///     pub Timelines {
///         /// The main timeline.
///         MY_TIMELINE = "timeline-123",
///         OTHER_TIMELINE = "timeline-456",
///     }
/// }
/// ```
///
/// Rust and Hack both get `Timelines::MY_TIMELINE`, TypeScript gets
/// `Timelines.MY_TIMELINE`. Flow gets the union of the values only, because
/// `.js.flow` files are declarations and cannot carry runtime values.
///
/// Values are string or integer literals. An integer group becomes `i64` in
/// Rust and a Hack `enum ...: int as int`, so a threshold can be declared here
/// and compared against without going through a string. A group may not mix the
/// two — a Hack enum has a single base type.
///
/// Group-level `#[typegen(skip(..))]` and `#[typegen(Hack("..")]`-style
/// overrides work exactly as they do on `#[derive(TypeGen)]`.
#[proc_macro]
pub fn typegen_consts(input: TokenStream) -> TokenStream {
    let group = parse_macro_input!(input as ConstGroup);

    expand_const_group(group)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(TypeGen, attributes(typegen))]
pub fn type_gen(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let struct_docs = extract_docs(&input.attrs);

    match &input.data {
        Data::Struct(data_struct) => {
            match &data_struct.fields {
                Fields::Named(fields_named) => {
                    // Regular struct with named fields
                    let fields = fields_named
                        .named
                        .iter()
                        .filter_map(|field| {
                            // Check if this field should be skipped entirely
                            if extract_typegen_skip_all(&field.attrs) {
                                return None;
                            }

                            let field_name = field.ident.as_ref().unwrap().to_string();
                            let field_type = &field.ty;
                            let field_docs = extract_docs(&field.attrs);
                            let type_override = extract_typegen_as(&field.attrs);

                            let docs_token = match field_docs {
                                Some(docs) => quote! { Some(#docs.to_string()) },
                                None => quote! { None },
                            };

                            let type_ref_token = generate_type_ref_token(type_override, field_type);

                            Some(quote! {
                                ::typegen::FieldDeclaration {
                                    field_name: #field_name.to_string(),
                                    docs: #docs_token,
                                    type_ref: #type_ref_token,
                                }
                            })
                        })
                        .collect::<Vec<_>>();

                    // Generate field validation tests
                    let field_tests = generate_field_validation_tests(name, &input.data);

                    let docs_token = match struct_docs {
                        Some(docs) => quote! { Some(#docs.to_string()) },
                        None => quote! { None },
                    };

                    let overrides_token = extract_type_overrides(&input.attrs);
                    let skip_token = extract_type_skip(&input.attrs);

                    let expanded = quote! {
                        impl ::typegen::TypeGenDeclTrait for #name {
                            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                                ::typegen::TypeGenGeneratedType {
                                    original_type_name: stringify!(#name).to_string(),
                                    docs: #docs_token,
                                    file_path: std::path::PathBuf::from(file!()),
                                    declaration: ::typegen::TypeGenDecl::StructDecl(::typegen::StructDecl {
                                        fields: vec![#(#fields),*],
                                    }),
                                    overrides: #overrides_token,
                                    skip: #skip_token,
                                }
                            }
                        }

                        impl ::typegen::TypeGenTypeRefTrait for #name {
                            fn type_ref() -> ::typegen::TypeRef {
                                ::typegen::TypeRef::TypeReference(stringify!(#name).to_string())
                            }
                        }

                        #field_tests
                    };

                    TokenStream::from(expanded)
                }
                Fields::Unnamed(fields_unnamed) => {
                    // Tuple struct
                    let field_types = fields_unnamed
                        .unnamed
                        .iter()
                        .map(|field| {
                            let field_type = &field.ty;
                            let type_override = extract_typegen_as(&field.attrs);

                            generate_type_ref_token(type_override, field_type)
                        })
                        .collect::<Vec<_>>();

                    // Generate field validation tests
                    let field_tests = generate_field_validation_tests(name, &input.data);

                    let docs_token = match struct_docs {
                        Some(docs) => quote! { Some(#docs.to_string()) },
                        None => quote! { None },
                    };

                    let overrides_token = extract_type_overrides(&input.attrs);
                    let skip_token = extract_type_skip(&input.attrs);

                    let expanded = quote! {
                        impl ::typegen::TypeGenDeclTrait for #name {
                            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                                ::typegen::TypeGenGeneratedType {
                                    original_type_name: stringify!(#name).to_string(),
                                    docs: #docs_token,
                                    file_path: std::path::PathBuf::from(file!()),
                                    declaration: ::typegen::TypeGenDecl::TupleStructDecl(::typegen::TupleStructDecl {
                                        fields: vec![#(#field_types),*],
                                    }),
                                    overrides: #overrides_token,
                                    skip: #skip_token,
                                }
                            }
                        }

                        impl ::typegen::TypeGenTypeRefTrait for #name {
                            fn type_ref() -> ::typegen::TypeRef {
                                ::typegen::TypeRef::TypeReference(stringify!(#name).to_string())
                            }
                        }

                        #field_tests
                    };

                    TokenStream::from(expanded)
                }
                Fields::Unit => {
                    // Generate field validation tests (empty for unit structs)
                    let field_tests = generate_field_validation_tests(name, &input.data);

                    let docs_token = match struct_docs {
                        Some(docs) => quote! { Some(#docs.to_string()) },
                        None => quote! { None },
                    };

                    let overrides_token = extract_type_overrides(&input.attrs);
                    let skip_token = extract_type_skip(&input.attrs);

                    let expanded = quote! {
                        impl ::typegen::TypeGenDeclTrait for #name {
                            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                                ::typegen::TypeGenGeneratedType {
                                    original_type_name: stringify!(#name).to_string(),
                                    docs: #docs_token,
                                    file_path: std::path::PathBuf::from(file!()),
                                    declaration: ::typegen::TypeGenDecl::Null,
                                    overrides: #overrides_token,
                                    skip: #skip_token,
                                }
                            }
                        }

                        impl ::typegen::TypeGenTypeRefTrait for #name {
                            fn type_ref() -> ::typegen::TypeRef {
                                ::typegen::TypeRef::TypeReference(stringify!(#name).to_string())
                            }
                        }

                        #field_tests
                    };

                    TokenStream::from(expanded)
                }
            }
        }
        Data::Enum(data_enum) => {
            let docs_token = match struct_docs {
                Some(docs) => quote! { Some(#docs.to_string()) },
                None => quote! { None },
            };

            let overrides_token = extract_type_overrides(&input.attrs);
            let skip_token = extract_type_skip(&input.attrs);

            // Process enum variants
            let variants = data_enum
                .variants
                .iter()
                .map(|variant| {
                    let variant_name = variant.ident.to_string();
                    let variant_docs = extract_docs(&variant.attrs);
                    let docs_token = match variant_docs {
                        Some(docs) => quote! { Some(#docs.to_string()) },
                        None => quote! { None },
                    };

                    match &variant.fields {
                        syn::Fields::Unit => {
                            // Unit variant: Variant
                            quote! {
                                ::typegen::EnumVariant::Unit {
                                    name: #variant_name.to_string(),
                                    docs: #docs_token,
                                }
                            }
                        }
                        syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                            // Newtype variant: Variant(Type)
                            let field = &fields.unnamed[0];
                            let field_type = &field.ty;
                            let type_override = extract_typegen_as(&field.attrs);

                            let type_ref_token = generate_type_ref_token(type_override, field_type);

                            quote! {
                                ::typegen::EnumVariant::Newtype {
                                    name: #variant_name.to_string(),
                                    docs: #docs_token,
                                    field_type: #type_ref_token,
                                }
                            }
                        }
                        syn::Fields::Unnamed(fields) => {
                            // Tuple variant: Variant(Type1, Type2, ...)
                            let field_types = fields.unnamed.iter().map(|field| {
                                let field_type = &field.ty;
                                let type_override = extract_typegen_as(&field.attrs);

                                generate_type_ref_token(type_override, field_type)
                            });
                            quote! {
                                ::typegen::EnumVariant::Tuple {
                                    name: #variant_name.to_string(),
                                    docs: #docs_token,
                                    fields: vec![#(#field_types),*],
                                }
                            }
                        }
                        syn::Fields::Named(fields) => {
                            // Struct variant: Variant { field1: Type1, field2: Type2, ... }
                            let struct_fields = fields.named.iter().filter_map(|field| {
                                // Check if this field should be skipped entirely
                                if extract_typegen_skip_all(&field.attrs) {
                                    return None;
                                }

                                let field_name = field.ident.as_ref().unwrap().to_string();
                                let field_type = &field.ty;
                                let field_docs = extract_docs(&field.attrs);
                                let type_override = extract_typegen_as(&field.attrs);

                                let field_docs_token = match field_docs {
                                    Some(docs) => quote! { Some(#docs.to_string()) },
                                    None => quote! { None },
                                };

                                let type_ref_token =
                                    generate_type_ref_token(type_override, field_type);

                                Some(quote! {
                                    ::typegen::FieldDeclaration {
                                        field_name: #field_name.to_string(),
                                        type_ref: #type_ref_token,
                                        docs: #field_docs_token,
                                    }
                                })
                            });
                            quote! {
                                ::typegen::EnumVariant::Struct {
                                    name: #variant_name.to_string(),
                                    docs: #docs_token,
                                    fields: vec![#(#struct_fields),*],
                                }
                            }
                        }
                    }
                })
                .collect::<Vec<_>>();

            // Validate: mixed enums (some unit, some data variants) are not allowed.
            // Either all variants must be unit variants, or all must carry data.
            {
                let has_unit = data_enum
                    .variants
                    .iter()
                    .any(|v| matches!(v.fields, syn::Fields::Unit));
                let has_data = data_enum
                    .variants
                    .iter()
                    .any(|v| !matches!(v.fields, syn::Fields::Unit));
                if has_unit && has_data {
                    let unit_variants: Vec<_> = data_enum
                        .variants
                        .iter()
                        .filter(|v| matches!(v.fields, syn::Fields::Unit))
                        .map(|v| v.ident.to_string())
                        .collect();
                    let data_variants: Vec<_> = data_enum
                        .variants
                        .iter()
                        .filter(|v| !matches!(v.fields, syn::Fields::Unit))
                        .map(|v| v.ident.to_string())
                        .collect();
                    return syn::Error::new_spanned(
                        &input,
                        format!(
                            "TypeGen does not support mixed enums (some unit variants, some data variants). \
                             Either all variants must be unit variants or all must carry data.\n\
                             Unit variants: [{}]\n\
                             Data variants: [{}]",
                            unit_variants.join(", "),
                            data_variants.join(", "),
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
            }

            // Generate field validation tests for enums
            let field_tests = generate_field_validation_tests(name, &input.data);

            let expanded = quote! {
                impl ::typegen::TypeGenDeclTrait for #name {
                    fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                        ::typegen::TypeGenGeneratedType {
                            original_type_name: stringify!(#name).to_string(),
                            docs: #docs_token,
                            file_path: std::path::PathBuf::from(file!()),
                            declaration: ::typegen::TypeGenDecl::EnumDecl(::typegen::EnumDecl {
                                variants: vec![#(#variants),*],
                            }),
                            overrides: #overrides_token,
                            skip: #skip_token,
                        }
                    }
                }

                impl ::typegen::TypeGenTypeRefTrait for #name {
                    fn type_ref() -> ::typegen::TypeRef {
                        ::typegen::TypeRef::TypeReference(stringify!(#name).to_string())
                    }
                }

                #field_tests
            };

            TokenStream::from(expanded)
        }
        Data::Union(_) => {
            // Unions not supported
            syn::Error::new_spanned(&input, "Unions are not supported")
                .to_compile_error()
                .into()
        }
    }
}

/// A parsed `typegen_consts!` group: `pub Timelines { NAME = "value", .. }`
struct ConstGroup {
    attrs: Vec<syn::Attribute>,
    visibility: syn::Visibility,
    name: syn::Ident,
    entries: Vec<ConstGroupEntry>,
}

/// A single `NAME = <literal>` line within a [`ConstGroup`]
struct ConstGroupEntry {
    attrs: Vec<syn::Attribute>,
    name: syn::Ident,
    value: ConstGroupValue,
}

/// The literal on the right of a [`ConstGroupEntry`].
///
/// Strings and integers only: they are the two kinds every target language
/// spells the same way, and the two a Hack enum can be based on. An integer is
/// normalized to `i64` here rather than carried as its `LitInt`, so a suffixed
/// `1000u32` cannot end up on the right of a generated `const NAME: i64`.
enum ConstGroupValue {
    Str(syn::LitStr),
    Int(i64),
}

impl ConstGroupValue {
    fn is_int(&self) -> bool {
        matches!(self, ConstGroupValue::Int(_))
    }
}

impl Parse for ConstGroup {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let visibility: syn::Visibility = input.parse()?;
        let name: syn::Ident = input.parse()?;

        let body;
        syn::braced!(body in input);
        let entries = body
            .parse_terminated(ConstGroupEntry::parse, syn::Token![,])?
            .into_iter()
            .collect();

        Ok(ConstGroup {
            attrs,
            visibility,
            name,
            entries,
        })
    }
}

impl Parse for ConstGroupEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let name: syn::Ident = input.parse()?;
        input.parse::<syn::Token![=]>()?;
        let value: ConstGroupValue = input.parse()?;

        Ok(ConstGroupEntry { attrs, name, value })
    }
}

impl Parse for ConstGroupValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        match input.parse::<syn::Lit>()? {
            syn::Lit::Str(value) => Ok(ConstGroupValue::Str(value)),
            // Overflow is reported here, against the literal, rather than
            // surfacing later as a type error inside the generated `const`.
            syn::Lit::Int(value) => {
                let parsed: i64 = value.base10_parse()?;
                reject_unsafe_integer(parsed, &value)?;
                Ok(ConstGroupValue::Int(parsed))
            }
            other => Err(syn::Error::new_spanned(
                other,
                "typegen_consts! values must be a string or an integer literal",
            )),
        }
    }
}

fn expand_const_group(group: ConstGroup) -> syn::Result<proc_macro2::TokenStream> {
    let ConstGroup {
        attrs,
        visibility,
        name,
        entries,
    } = group;

    if entries.is_empty() {
        return Err(syn::Error::new(
            name.span(),
            "typegen_consts! group must declare at least one constant",
        ));
    }

    reject_unknown_attrs(&attrs, &["doc", "typegen"])?;
    for entry in &entries {
        reject_unknown_attrs(&entry.attrs, &["doc"])?;
    }
    reject_mixed_values(&entries)?;

    let group_name = name.to_string();
    // Only `#[doc]` survives onto the generated struct — `#[typegen(..)]` is a
    // helper attribute registered by the derive macro and is not in scope here.
    let group_docs = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .collect::<Vec<_>>();
    let group_docs_token = docs_token(extract_docs(&attrs));
    let overrides_token = extract_type_overrides(&attrs);
    let skip_token = extract_type_skip(&attrs);

    let consts = entries
        .iter()
        .map(|entry| {
            let entry_docs = &entry.attrs;
            let entry_name = &entry.name;
            match &entry.value {
                ConstGroupValue::Str(value) => quote! {
                    #(#entry_docs)*
                    #visibility const #entry_name: &'static str = #value;
                },
                ConstGroupValue::Int(value) => quote! {
                    #(#entry_docs)*
                    #visibility const #entry_name: i64 = #value;
                },
            }
        })
        .collect::<Vec<_>>();

    let decl_entries = entries
        .iter()
        .map(|entry| {
            let entry_name = entry.name.to_string();
            let value = match &entry.value {
                ConstGroupValue::Str(value) => {
                    let value = value.value();
                    quote! { ::typegen::ConstValue::Str(#value.to_string()) }
                }
                ConstGroupValue::Int(value) => quote! { ::typegen::ConstValue::Int(#value) },
            };
            let entry_docs_token = docs_token(extract_docs(&entry.attrs));
            quote! {
                ::typegen::ConstEntry {
                    name: #entry_name.to_string(),
                    value: #value,
                    docs: #entry_docs_token,
                }
            }
        })
        .collect::<Vec<_>>();

    let test_name = syn::Ident::new(
        &format!("test_{}_const_generation", group_name.to_lowercase()),
        name.span(),
    );

    Ok(quote! {
        #(#group_docs)*
        #visibility struct #name;

        impl #name {
            #(#consts)*
        }

        impl ::typegen::TypeGenTypeRefTrait for #name {
            fn type_ref() -> ::typegen::TypeRef {
                ::typegen::TypeRef::TypeReference(#group_name.to_string())
            }
        }

        impl ::typegen::TypeGenDeclTrait for #name {
            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                ::typegen::TypeGenGeneratedType {
                    original_type_name: #group_name.to_string(),
                    docs: #group_docs_token,
                    file_path: std::path::PathBuf::from(file!()),
                    declaration: ::typegen::TypeGenDecl::ConstDecl(::typegen::ConstDecl {
                        entries: vec![#(#decl_entries),*],
                    }),
                    overrides: #overrides_token,
                    skip: #skip_token,
                }
            }
        }

        #[cfg(test)]
        #[test]
        fn #test_name() {
            // Generate and write files for this const group
            let type_decl = <#name as ::typegen::TypeGenDeclTrait>::to_type_decl();
            type_decl.write_to_file().expect("Failed to write const files");
        }
    })
}

/// Largest integer a JS `number` holds exactly — `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Reject an integer the JS targets could not represent exactly.
///
/// Rust and Hack carry the full `i64`, but Flow and TypeScript get a `number`,
/// which is a double. Above this the emitted literal rounds to a *different*
/// value, and nothing downstream fails — the constant is simply not the same
/// number in two of the four languages, which is the exact failure this crate
/// exists to prevent.
fn reject_unsafe_integer(value: i64, literal: &syn::LitInt) -> syn::Result<()> {
    if value.unsigned_abs() > MAX_SAFE_INTEGER as u64 {
        return Err(syn::Error::new_spanned(
            literal,
            format!(
                "typegen_consts! integer {value} is outside the range a JS number \
                 holds exactly (±{MAX_SAFE_INTEGER}); it would round to a different \
                 value in the Flow and TypeScript output",
            ),
        ));
    }

    Ok(())
}

/// Reject a group that mixes string and integer values.
///
/// A Hack enum has one base type and there is no `string|int` to widen to, so a
/// mixed group has no legal Hack spelling. Caught here, against the offending
/// line, rather than emitting an enum whose values do not match its base type —
/// which the Hack typechecker would report far from the `typegen_consts!` that
/// caused it.
fn reject_mixed_values(entries: &[ConstGroupEntry]) -> syn::Result<()> {
    let Some(first) = entries.first() else {
        return Ok(());
    };

    for entry in entries {
        if entry.value.is_int() != first.value.is_int() {
            return Err(syn::Error::new(
                entry.name.span(),
                format!(
                    "typegen_consts! group mixes string and integer values \
                     (`{}` disagrees with `{}`); split them into two groups",
                    entry.name, first.name,
                ),
            ));
        }
    }

    Ok(())
}

/// Reject attributes the macro would otherwise drop on the floor.
fn reject_unknown_attrs(attrs: &[syn::Attribute], allowed: &[&str]) -> syn::Result<()> {
    for attr in attrs {
        if !allowed.iter().any(|name| attr.path().is_ident(name)) {
            return Err(syn::Error::new_spanned(
                attr,
                format!(
                    "unsupported attribute in typegen_consts!; only {} allowed here",
                    allowed
                        .iter()
                        .map(|name| format!("`#[{name}]`"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
            ));
        }
    }
    Ok(())
}

fn docs_token(docs: Option<String>) -> proc_macro2::TokenStream {
    match docs {
        Some(docs) => quote! { Some(#docs.to_string()) },
        None => quote! { None },
    }
}

fn extract_docs(attrs: &[syn::Attribute]) -> Option<String> {
    let mut docs = Vec::new();

    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(syn::MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(lit_str),
                        ..
                    }),
                ..
            }) = &attr.meta
            {
                let doc_str = lit_str.value();
                // Remove leading space that rustc adds to doc comments
                let trimmed = doc_str.strip_prefix(' ').unwrap_or(&doc_str);
                docs.push(trimmed.to_string());
            }
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

fn extract_typegen_skip_all(attrs: &[syn::Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("typegen") {
            if let syn::Meta::List(meta_list) = &attr.meta {
                let tokens_str = meta_list.tokens.to_string();

                // Look for skip_all pattern
                if tokens_str.contains("skip_all") {
                    return true;
                }
            }
        }
    }
    false
}

fn extract_typegen_as(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("typegen") {
            if let syn::Meta::List(meta_list) = &attr.meta {
                // Parse the meta list as a sequence of nested metas
                let nested_meta: Result<syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>, _> =
                    meta_list.parse_args_with(syn::punctuated::Punctuated::parse_terminated);

                if let Ok(nested_metas) = nested_meta {
                    for meta in nested_metas {
                        if let syn::Meta::NameValue(name_value) = meta {
                            if name_value.path.is_ident("as") {
                                if let syn::Expr::Lit(syn::ExprLit {
                                    lit: syn::Lit::Str(lit_str),
                                    ..
                                }) = name_value.value
                                {
                                    // Found the attribute! Return the value
                                    return Some(lit_str.value());
                                }
                            }
                        }
                    }
                } else {
                    // If parsing failed, let's try a fallback approach
                    // Just look for the pattern as = "value" in the tokens
                    let tokens_str = meta_list.tokens.to_string();
                    if tokens_str.contains("as = ") {
                        // Simple string matching as fallback
                        if let Some(start) = tokens_str.find("as = \"") {
                            let value_start = start + 6; // Skip 'as = "'
                            if let Some(end) = tokens_str[value_start..].find('"') {
                                let value = &tokens_str[value_start..value_start + end];
                                return Some(value.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_type_overrides(attrs: &[syn::Attribute]) -> proc_macro2::TokenStream {
    let mut hack_override = None;
    let mut flow_override = None;
    let mut typescript_override = None;

    for attr in attrs {
        if attr.path().is_ident("typegen") {
            if let syn::Meta::List(meta_list) = &attr.meta {
                // Parse the meta list as function calls like Hack("null"), TypeScript("() -> {}")
                let tokens_str = meta_list.tokens.to_string();

                // Look for Hack("...") pattern
                if let Some(hack_match) = extract_language_override(&tokens_str, "Hack") {
                    hack_override = Some(hack_match);
                }

                // Look for Flow("...") pattern
                if let Some(flow_match) = extract_language_override(&tokens_str, "Flow") {
                    flow_override = Some(flow_match);
                }

                // Look for TypeScript("...") pattern
                if let Some(ts_match) = extract_language_override(&tokens_str, "TypeScript") {
                    typescript_override = Some(ts_match);
                }
            }
        }
    }

    // If no overrides were found, return None
    if hack_override.is_none() && flow_override.is_none() && typescript_override.is_none() {
        return quote! { None };
    }

    let hack_token = match hack_override {
        Some(value) => quote! { Some(#value) },
        None => quote! { None },
    };
    let flow_token = match flow_override {
        Some(value) => quote! { Some(#value) },
        None => quote! { None },
    };
    let typescript_token = match typescript_override {
        Some(value) => quote! { Some(#value) },
        None => quote! { None },
    };

    quote! {
        Some(::typegen::TypeGenOverrides {
            hack: #hack_token,
            flow: #flow_token,
            typescript: #typescript_token,
        })
    }
}

fn extract_type_skip(attrs: &[syn::Attribute]) -> proc_macro2::TokenStream {
    let mut skip_hack = false;
    let mut skip_flow = false;
    let mut skip_typescript = false;

    for attr in attrs {
        if attr.path().is_ident("typegen") {
            if let syn::Meta::List(meta_list) = &attr.meta {
                let tokens_str = meta_list.tokens.to_string();

                // Look for skip(...) pattern
                let skip_languages = extract_skip_languages(&tokens_str);
                skip_hack = skip_languages.contains(&"Hack".to_string());
                skip_flow = skip_languages.contains(&"Flow".to_string());
                skip_typescript = skip_languages.contains(&"TypeScript".to_string());
            }
        }
    }

    // If no skip flags were found, return None
    if !skip_hack && !skip_flow && !skip_typescript {
        return quote! { None };
    }

    quote! {
        Some(::typegen::TypeGenSkip {
            hack: #skip_hack,
            flow: #skip_flow,
            typescript: #skip_typescript,
        })
    }
}

fn extract_language_override(tokens_str: &str, language: &str) -> Option<String> {
    let pattern = format!("{language}(\"");
    if let Some(start) = tokens_str.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = tokens_str[value_start..].find("\")") {
            let value = &tokens_str[value_start..value_start + end];
            return Some(value.to_string());
        }
    }
    None
}

fn extract_skip_languages(tokens_str: &str) -> Vec<String> {
    let mut languages = Vec::new();
    const VALID_LANGUAGES: &[&str] = &["Hack", "Flow", "TypeScript"];

    // Look for skip(...) pattern
    if let Some(start) = tokens_str.find("skip(") {
        let value_start = start + 5; // "skip(".len()
        if let Some(end) = tokens_str[value_start..].find(")") {
            let content = &tokens_str[value_start..value_start + end];

            // Split by comma and extract language names
            for part in content.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    if VALID_LANGUAGES.contains(&trimmed) {
                        languages.push(trimmed.to_string());
                    } else {
                        panic!(
                            "Invalid language '{}' in skip attribute. Only the following options are available: {}",
                            trimmed,
                            VALID_LANGUAGES.join(", ")
                        );
                    }
                }
            }
        }
    }

    languages
}

fn generate_type_ref_token(
    type_override: Option<String>,
    field_type: &syn::Type,
) -> proc_macro2::TokenStream {
    match type_override {
        Some(override_type) => {
            // Parse the override type as a Rust type and use its type_ref() implementation
            let type_str = override_type.as_str();
            let parsed_type: syn::Type = syn::parse_str(type_str).unwrap_or_else(|_| {
                panic!("Invalid type in #[typegen(as = \"{type_str}\")] attribute")
            });
            quote! { <#parsed_type as ::typegen::TypeGenTypeRefTrait>::type_ref() }
        }
        None => quote! { <#field_type as ::typegen::TypeGenTypeRefTrait>::type_ref() },
    }
}

fn generate_field_validation_tests(name: &syn::Ident, data: &Data) -> proc_macro2::TokenStream {
    match data {
        Data::Struct(_) => {
            let test_name = syn::Ident::new(
                &format!("test_{}_type_generation", name.to_string().to_lowercase()),
                name.span(),
            );
            quote! {
                #[cfg(test)]
                #[test]
                fn #test_name() {
                    // Generate and write TypeScript/Flow files for this type
                    let type_decl = <#name as ::typegen::TypeGenDeclTrait>::to_type_decl();
                    type_decl.write_to_file().expect("Failed to write type files");
                }
            }
        }
        Data::Enum(_) => {
            let test_name = syn::Ident::new(
                &format!("test_{}_type_generation", name.to_string().to_lowercase()),
                name.span(),
            );
            quote! {
                #[cfg(test)]
                #[test]
                fn #test_name() {
                    // Generate and write TypeScript/Flow files for this enum type
                    let type_decl = <#name as ::typegen::TypeGenDeclTrait>::to_type_decl();
                    type_decl.write_to_file().expect("Failed to write type files");
                }
            }
        }
        _ => quote! {},
    }
}
