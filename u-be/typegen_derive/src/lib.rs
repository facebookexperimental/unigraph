#![allow(clippy::single_match)]
#![allow(clippy::collapsible_if)]

use proc_macro::TokenStream;
use quote::quote;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::parse_macro_input;

#[proc_macro_derive(TypeGen)]
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
                        .map(|field| {
                            let field_name = field.ident.as_ref().unwrap().to_string();
                            let field_type = &field.ty;
                            let field_docs = extract_docs(&field.attrs);
                            let docs_token = match field_docs {
                                Some(docs) => quote! { Some(#docs.to_string()) },
                                None => quote! { None },
                            };
                            quote! {
                                ::typegen::FieldDeclaration {
                                    field_name: #field_name.to_string(),
                                    type_ref: <#field_type as ::typegen::TypeGenTypeRefTrait>::type_ref(),
                                    docs: #docs_token,
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    // Generate field validation tests
                    let field_tests = generate_field_validation_tests(name, &input.data);

                    let docs_token = match struct_docs {
                        Some(docs) => quote! { Some(#docs.to_string()) },
                        None => quote! { None },
                    };

                    let expanded = quote! {
                        impl ::typegen::TypeGenDeclTrait for #name {
                            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                                ::typegen::TypeGenGeneratedType {
                                    type_name: stringify!(#name).to_string(),
                                    docs: #docs_token,
                                    file_path: ::typegen::SourceFilePath(std::path::PathBuf::from(file!())),
                                    declaration: ::typegen::TypeGenDecl::StructDecl(::typegen::StructDecl {
                                        fields: vec![#(#fields),*],
                                    })
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
                            quote! { <#field_type as ::typegen::TypeGenTypeRefTrait>::type_ref() }
                        })
                        .collect::<Vec<_>>();

                    // Generate field validation tests
                    let field_tests = generate_field_validation_tests(name, &input.data);

                    let docs_token = match struct_docs {
                        Some(docs) => quote! { Some(#docs.to_string()) },
                        None => quote! { None },
                    };

                    let expanded = quote! {
                        impl ::typegen::TypeGenDeclTrait for #name {
                            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                                ::typegen::TypeGenGeneratedType {
                                    type_name: stringify!(#name).to_string(),
                                    docs: #docs_token,
                                    file_path: ::typegen::SourceFilePath(std::path::PathBuf::from(file!())),
                                    declaration: ::typegen::TypeGenDecl::TupleStructDecl(::typegen::TupleStructDecl {
                                        fields: vec![#(#field_types),*],
                                    })
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

                    let expanded = quote! {
                        impl ::typegen::TypeGenDeclTrait for #name {
                            fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                                ::typegen::TypeGenGeneratedType {
                                    type_name: stringify!(#name).to_string(),
                                    docs: #docs_token,
                                    file_path: ::typegen::SourceFilePath(std::path::PathBuf::from(file!())),
                                    declaration: ::typegen::TypeGenDecl::Null
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

            // Process enum variants
            let variants = data_enum.variants.iter().map(|variant| {
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
                        let field_type = &fields.unnamed[0].ty;
                        quote! {
                            ::typegen::EnumVariant::Newtype {
                                name: #variant_name.to_string(),
                                docs: #docs_token,
                                field_type: <#field_type as ::typegen::TypeGenTypeRefTrait>::type_ref(),
                            }
                        }
                    }
                    syn::Fields::Unnamed(fields) => {
                        // Tuple variant: Variant(Type1, Type2, ...)
                        let field_types = fields.unnamed.iter().map(|field| {
                            let field_type = &field.ty;
                            quote! { <#field_type as ::typegen::TypeGenTypeRefTrait>::type_ref() }
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
                        let struct_fields = fields.named.iter().map(|field| {
                            let field_name = field.ident.as_ref().unwrap().to_string();
                            let field_type = &field.ty;
                            let field_docs = extract_docs(&field.attrs);
                            let field_docs_token = match field_docs {
                                Some(docs) => quote! { Some(#docs.to_string()) },
                                None => quote! { None },
                            };
                            quote! {
                                ::typegen::FieldDeclaration {
                                    field_name: #field_name.to_string(),
                                    type_ref: <#field_type as ::typegen::TypeGenTypeRefTrait>::type_ref(),
                                    docs: #field_docs_token,
                                }
                            }
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
            }).collect::<Vec<_>>();

            let expanded = quote! {
                impl ::typegen::TypeGenDeclTrait for #name {
                    fn to_type_decl() -> ::typegen::TypeGenGeneratedType {
                        ::typegen::TypeGenGeneratedType {
                            type_name: stringify!(#name).to_string(),
                            docs: #docs_token,
                            file_path: ::typegen::SourceFilePath(std::path::PathBuf::from(file!())),
                            declaration: ::typegen::TypeGenDecl::EnumDecl(::typegen::EnumDecl {
                                variants: vec![#(#variants),*],
                            })
                        }
                    }
                }

                impl ::typegen::TypeGenTypeRefTrait for #name {
                    fn type_ref() -> ::typegen::TypeRef {
                        ::typegen::TypeRef::TypeReference(stringify!(#name).to_string())
                    }
                }
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
            // TODO: Add enum tests when enum support is implemented
            quote! {}
        }
        _ => quote! {},
    }
}
