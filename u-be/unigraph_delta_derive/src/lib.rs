// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Derive macro for the `Deltable` trait.
//!
//! # Modes
//!
//! ## `#[derive(Deltable)]` (default — field-level diffing)
//!
//! For structs with named fields and for enums. Generates a companion
//! `{TypeName}Delta` type and a `Deltable` impl that diffs sub-structure
//! independently, so only what actually changed appears in the serialized delta.
//!
//! - **Structs with named fields**: generates a `{StructName}Delta` struct where
//!   each field `foo: T` becomes `foo: Option<<T as Deltable>::Delta>`. Only
//!   changed fields are serialized.
//! - **Enums**: generates a `{EnumName}Delta` enum. When both sides are the same
//!   variant, the delta carries per-field deltas for that variant (same shape as
//!   the struct path). When the variant changes, the delta is a `Replace(Self)`
//!   arm carrying the full new value.
//!
//! ## `#[derive(Deltable)] #[deltable(replace)]` (whole-value replacement)
//!
//! For any type (struct, enum, tuple struct). The delta IS the full replacement
//! value (`Delta = Self`). No companion type is generated. Use this for types
//! where sub-field diffing doesn't make sense — small enums/structs that should
//! always be replaced as a unit.
//!
//! Requires `PartialEq + Clone` on the type.

use proc_macro::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Data;
use syn::DataEnum;
use syn::DeriveInput;
use syn::Fields;
use syn::Ident;
use syn::Visibility;
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::token::Comma;

/// Derive `Deltable` for a type.
///
/// By default (no attributes), generates field-level diffing for structs with
/// named fields and for enums. Add `#[deltable(replace)]` for whole-value
/// replacement mode.
///
/// # Field-level diffing (default)
///
/// Generates a companion `{TypeName}Delta` type and a `Deltable` impl with
/// `derive_delta`, `apply_delta`, and `merge_delta`. The generated delta type
/// has the same visibility as the source type.
///
/// # Whole-value replacement (`#[deltable(replace)]`)
///
/// Works on any type (struct, enum, tuple struct). Generates a `Deltable` impl
/// where `Delta = Self` — the delta is just the new value. No companion delta
/// type is generated. The type must implement `PartialEq` and `Clone`.
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

/// Dispatch field-level diffing based on the data shape.
fn derive_field_level(input: &DeriveInput) -> TokenStream {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => derive_field_level_struct(input, &fields.named),
            _ => syn::Error::new_spanned(
                input,
                "Deltable can only be derived for structs with named fields \
                 (use #[deltable(replace)] for tuple/unit structs)",
            )
            .to_compile_error()
            .into(),
        },
        Data::Enum(data) => derive_field_level_enum(input, data),
        Data::Union(_) => syn::Error::new_spanned(
            input,
            "Deltable cannot be derived for unions (use #[deltable(replace)])",
        )
        .to_compile_error()
        .into(),
    }
}

/// Visibility tokens for the generated delta type (mirrors the source type).
fn delta_visibility(vis: &Visibility) -> impl quote::ToTokens {
    match vis {
        Visibility::Public(_) => quote! { pub },
        Visibility::Restricted(r) => quote! { #r },
        Visibility::Inherited => quote! {},
    }
}

/// Generate field-level diffing `Deltable` impl with companion `*Delta` struct.
fn derive_field_level_struct(
    input: &DeriveInput,
    fields: &Punctuated<syn::Field, Comma>,
) -> TokenStream {
    let name = &input.ident;
    let delta_name = format_ident!("{}Delta", name);

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

    let delta_vis = delta_visibility(&input.vis);

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

/// Generate field-level diffing `Deltable` impl with companion `*Delta` enum.
///
/// The generated `{Name}Delta` enum has:
/// - one variant per source variant carrying per-field deltas (same-variant
///   change), omitted for unit variants which can never change in place, and
/// - a `Replace(Self)` variant carrying the full new value for cross-variant
///   changes.
fn derive_field_level_enum(input: &DeriveInput, data: &DataEnum) -> TokenStream {
    let name = &input.ident;
    let delta_name = format_ident!("{}Delta", name);

    let mut delta_variant_decls = Vec::new();
    let mut derive_arms = Vec::new();
    let mut apply_arms = Vec::new();
    let mut merge_arms = Vec::new();

    for variant in &data.variants {
        let vname = &variant.ident;
        let mismatch_msg = format!(
            "Deltable: delta variant `{vname}` does not match the current value's variant",
        );

        match &variant.fields {
            Fields::Unit => {
                // A unit variant has no payload, so the same variant on both
                // sides is always unchanged. No delta variant is generated;
                // cross-variant changes go through `Replace`.
                derive_arms.push(quote! {
                    (Self::#vname, Self::#vname) => None,
                });
            }
            Fields::Unnamed(fields) => {
                let ftypes: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
                let n = ftypes.len();
                let self_binds = idents("__s", n);
                let other_binds = idents("__o", n);
                let delta_binds = idents("__d", n);
                let first_binds = idents("__f", n);
                let second_binds = idents("__g", n);

                delta_variant_decls.push(quote! {
                    #vname( #( Option<<#ftypes as ::unigraph_delta::Deltable>::Delta> ),* )
                });

                derive_arms.push(quote! {
                    (Self::#vname( #(#self_binds),* ), Self::#vname( #(#other_binds),* )) => {
                        #(
                            let #delta_binds = ::unigraph_delta::Deltable::derive_delta(
                                #self_binds, #other_binds,
                            );
                        )*
                        if #( #delta_binds.is_none() )&&* {
                            None
                        } else {
                            Some(#delta_name::#vname( #(#delta_binds),* ))
                        }
                    }
                });

                apply_arms.push(quote! {
                    #delta_name::#vname( #(#delta_binds),* ) => match self {
                        Self::#vname( #(#self_binds),* ) => {
                            #(
                                if let Some(__inner) = #delta_binds {
                                    ::unigraph_delta::Deltable::apply_delta(#self_binds, __inner)?;
                                }
                            )*
                        }
                        _ => ::anyhow::bail!(#mismatch_msg),
                    },
                });

                merge_arms.push(quote! {
                    (
                        #delta_name::#vname( #(#first_binds),* ),
                        #delta_name::#vname( #(#second_binds),* ),
                    ) => {
                        #delta_name::#vname(
                            #(
                                match (#first_binds, #second_binds) {
                                    (__x, None) => __x,
                                    (None, __y) => __y,
                                    (Some(__f), Some(__s)) => Some(
                                        <#ftypes as ::unigraph_delta::Deltable>::merge_delta(__f, __s)
                                    ),
                                }
                            ),*
                        )
                    }
                });
            }
            Fields::Named(fields) => {
                let fnames: Vec<Ident> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.clone().unwrap())
                    .collect();
                let ftypes: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
                let self_binds = suffixed_idents("__s", &fnames);
                let other_binds = suffixed_idents("__o", &fnames);
                let delta_binds = suffixed_idents("__d", &fnames);
                let first_binds = suffixed_idents("__f", &fnames);
                let second_binds = suffixed_idents("__g", &fnames);

                delta_variant_decls.push(quote! {
                    #vname { #(
                        #[serde(default, skip_serializing_if = "Option::is_none")]
                        #fnames: Option<<#ftypes as ::unigraph_delta::Deltable>::Delta>
                    ),* }
                });

                derive_arms.push(quote! {
                    (
                        Self::#vname { #(#fnames: #self_binds),* },
                        Self::#vname { #(#fnames: #other_binds),* },
                    ) => {
                        #(
                            let #delta_binds = ::unigraph_delta::Deltable::derive_delta(
                                #self_binds, #other_binds,
                            );
                        )*
                        if #( #delta_binds.is_none() )&&* {
                            None
                        } else {
                            Some(#delta_name::#vname { #(#fnames: #delta_binds),* })
                        }
                    }
                });

                apply_arms.push(quote! {
                    #delta_name::#vname { #(#fnames: #delta_binds),* } => match self {
                        Self::#vname { #(#fnames: #self_binds),* } => {
                            #(
                                if let Some(__inner) = #delta_binds {
                                    ::unigraph_delta::Deltable::apply_delta(#self_binds, __inner)?;
                                }
                            )*
                        }
                        _ => ::anyhow::bail!(#mismatch_msg),
                    },
                });

                merge_arms.push(quote! {
                    (
                        #delta_name::#vname { #(#fnames: #first_binds),* },
                        #delta_name::#vname { #(#fnames: #second_binds),* },
                    ) => {
                        #delta_name::#vname {
                            #(
                                #fnames: match (#first_binds, #second_binds) {
                                    (__x, None) => __x,
                                    (None, __y) => __y,
                                    (Some(__f), Some(__s)) => Some(
                                        <#ftypes as ::unigraph_delta::Deltable>::merge_delta(__f, __s)
                                    ),
                                }
                            ),*
                        }
                    }
                });
            }
        }
    }

    let delta_vis = delta_visibility(&input.vis);

    let expanded = quote! {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #[allow(clippy::large_enum_variant)]
        #delta_vis enum #delta_name {
            #(#delta_variant_decls,)*
            /// Cross-variant change: the full replacement value.
            Replace(#name),
        }

        #[allow(unreachable_patterns)]
        impl ::unigraph_delta::Deltable for #name {
            type Delta = #delta_name;

            fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
                match (self, other) {
                    #(#derive_arms)*
                    // Variant changed: store the full new value.
                    _ => Some(#delta_name::Replace(other.clone())),
                }
            }

            fn apply_delta(&mut self, delta: Self::Delta) -> ::anyhow::Result<()> {
                match delta {
                    #(#apply_arms)*
                    #delta_name::Replace(__v) => { *self = __v; }
                }
                Ok(())
            }

            fn merge_delta(first: Self::Delta, second: Self::Delta) -> Self::Delta {
                match (first, second) {
                    // Same variant on both deltas: merge field-by-field.
                    #(#merge_arms)*
                    // A later full replacement wins outright.
                    (_, __second @ #delta_name::Replace(_)) => __second,
                    // Earlier replacement, then an in-variant change: fold the
                    // change into the replacement value.
                    (#delta_name::Replace(mut __v), __other) => {
                        let _ = ::unigraph_delta::Deltable::apply_delta(&mut __v, __other);
                        #delta_name::Replace(__v)
                    }
                    // Mismatched in-variant deltas (not a valid chain): last wins.
                    (_, __second) => __second,
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generate `n` positional binding idents with the given prefix (`__s0`, ...).
fn idents(prefix: &str, n: usize) -> Vec<Ident> {
    (0..n).map(|i| format_ident!("{}{}", prefix, i)).collect()
}

/// Generate one binding ident per field name, prefixed (`__s_foo`, ...).
fn suffixed_idents(prefix: &str, names: &[Ident]) -> Vec<Ident> {
    names
        .iter()
        .map(|n| format_ident!("{}_{}", prefix, n))
        .collect()
}
