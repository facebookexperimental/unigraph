// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Derive macro for the `Deltable` trait.
//!
//! # Modes
//!
//! ## `#[derive(Deltable)]` (default — field-level diffing)
//!
//! For structs with named fields. Generates a companion `{StructName}Delta`
//! struct and a `Deltable` impl that diffs each field independently. Only
//! changed fields appear in the serialized delta.
//!
//! ## `#[derive(Deltable)] #[deltable(replace)]` (whole-value replacement)
//!
//! For any type (struct, enum, tuple struct). The delta IS the full replacement
//! value (`Delta = Self`). No companion struct is generated. Use this for types
//! where sub-field diffing doesn't make sense — enums, small structs, or types
//! that should always be replaced as a unit.
//!
//! Requires `PartialEq + Clone` on the type.

use proc_macro::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::Visibility;
use syn::parse_macro_input;

/// Derive `Deltable` for a type.
///
/// By default (no attributes), generates field-level diffing for structs with
/// named fields. Add `#[deltable(replace)]` for whole-value replacement mode.
///
/// # Field-level diffing (default)
///
/// Generates:
/// 1. A `{StructName}Delta` struct where each field `foo: T` becomes
///    `foo: Option<<T as Deltable>::Delta>`, with serde attributes to skip
///    unchanged (`None`) fields.
/// 2. A `Deltable` impl with `derive_delta` and `apply_delta`.
///
/// The generated delta struct has the same visibility as the source struct.
///
/// # Whole-value replacement (`#[deltable(replace)]`)
///
/// Works on any type (struct, enum, tuple struct). Generates a `Deltable` impl
/// where `Delta = Self` — the delta is just the new value. No companion delta
/// struct is generated. The type must implement `PartialEq` and `Clone`.
#[proc_macro_derive(Deltable, attributes(deltable))]
pub fn derive_deltable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    if has_replace_attr(&input) {
        derive_replace(&input)
    } else {
        derive_field_level(&input)
    }
}

/// Check if the type has `#[deltable(replace)]`.
fn has_replace_attr(input: &DeriveInput) -> bool {
    input.attrs.iter().any(|attr| {
        if !attr.path().is_ident("deltable") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("replace") {
                found = true;
            }
            Ok(())
        });
        found
    })
}

/// Generate a whole-value replacement `Deltable` impl (`Delta = Self`).
fn derive_replace(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;

    let expanded = quote! {
        impl ::unigraph_delta::Deltable for #name {
            type Delta = #name;

            fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
                if self == other { None } else { Some(other.clone()) }
            }

            fn apply_delta(&mut self, delta: Self::Delta) -> ::anyhow::Result<()> {
                *self = delta;
                Ok(())
            }

            fn merge_delta(_first: Self::Delta, second: Self::Delta) -> Self::Delta {
                // Whole-value replacement: last write wins.
                second
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generate field-level diffing `Deltable` impl with companion `*Delta` struct.
fn derive_field_level(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;
    let delta_name = format_ident!("{}Delta", name);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    input,
                    "Deltable can only be derived for structs with named fields \
                     (use #[deltable(replace)] for other types)",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                input,
                "Deltable can only be derived for structs \
                 (use #[deltable(replace)] for enums)",
            )
            .to_compile_error()
            .into();
        }
    };

    let delta_fields = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;
        let field_vis = &f.vis;
        quote! {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            #field_vis #field_name: Option<<#field_type as ::unigraph_delta::Deltable>::Delta>
        }
    });

    let derive_fields = fields.iter().map(|f| {
        let field_name = &f.ident;
        quote! {
            let #field_name = ::unigraph_delta::Deltable::derive_delta(
                &self.#field_name,
                &other.#field_name,
            );
        }
    });

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();

    let is_none_checks = field_names.iter().map(|name| {
        quote! { #name.is_none() }
    });

    let apply_fields = field_names.iter().map(|name| {
        quote! {
            if let Some(d) = delta.#name {
                ::unigraph_delta::Deltable::apply_delta(&mut self.#name, d)?;
            }
        }
    });

    let merge_fields = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_type = &f.ty;
        quote! {
            #field_name: match (first.#field_name, second.#field_name) {
                (first_val, None) => first_val,
                (None, second_val) => second_val,
                (Some(f), Some(s)) => Some(
                    <#field_type as ::unigraph_delta::Deltable>::merge_delta(f, s)
                ),
            }
        }
    });

    // Determine if we need to add pub to the generated struct module path
    let delta_vis = match vis {
        Visibility::Public(_) => quote! { pub },
        Visibility::Restricted(r) => quote! { #r },
        Visibility::Inherited => quote! {},
    };

    let expanded = quote! {
        #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
        #delta_vis struct #delta_name {
            #(#delta_fields,)*
        }

        impl ::unigraph_delta::Deltable for #name {
            type Delta = #delta_name;

            fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
                #(#derive_fields)*

                if #(#is_none_checks)&&* {
                    None
                } else {
                    Some(#delta_name {
                        #(#field_names,)*
                    })
                }
            }

            fn apply_delta(&mut self, delta: Self::Delta) -> ::anyhow::Result<()> {
                #(#apply_fields)*
                Ok(())
            }

            fn merge_delta(first: Self::Delta, second: Self::Delta) -> Self::Delta {
                #delta_name {
                    #(#merge_fields,)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
