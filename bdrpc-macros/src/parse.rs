//! Parsing logic for the `#[bdrpc::service]` macro.
//!
//! This module handles parsing trait definitions and extracting the information
//! needed to generate protocol enums, client stubs, and server dispatchers.

use proc_macro2::Span;
use syn::{
    Attribute, Error, Expr, ExprLit, FnArg, Ident, ItemTrait, Lit, Meta, MetaList, MetaNameValue,
    Pat, PatType, Result, ReturnType, TraitItem, TraitItemFn, Type, parse::Parser,
};

/// Direction of RPC communication (per ADR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Client can call, server responds only
    CallOnly,
    /// Server can call, client responds only
    RespondOnly,
    /// Both sides can call and respond (default)
    #[default]
    Bidirectional,
}

impl Direction {
    /// Parse direction from string literal.
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "call" | "call_only" | "callonly" => Ok(Direction::CallOnly),
            "respond" | "respond_only" | "respondonly" => Ok(Direction::RespondOnly),
            "bidirectional" | "bi" | "both" => Ok(Direction::Bidirectional),
            _ => Err(Error::new(
                Span::call_site(),
                format!(
                    "Invalid direction '{}'. Expected 'call', 'respond', or 'bidirectional'",
                    s
                ),
            )),
        }
    }
}

/// Parsed service definition.
#[derive(Debug)]
pub struct ServiceDef {
    /// The original trait
    #[allow(dead_code)]
    pub trait_def: ItemTrait,
    /// Service name (trait name)
    pub name: Ident,
    /// Communication direction
    pub direction: Direction,
    /// Protocol version (default: 1)
    pub version: u32,
    /// Minimum supported version (default: 1)
    pub min_version: u32,
    /// Optional feature flags
    pub features: Vec<String>,
    /// Service methods
    pub methods: Vec<MethodDef>,
}

/// Deprecation metadata for methods.
#[derive(Debug, Clone)]
pub struct DeprecationInfo {
    /// Version when deprecated
    pub since: String,
    /// Deprecation note/message
    pub note: Option<String>,
    /// Version when it will be removed
    pub remove_in: Option<String>,
}

/// Parsed method definition.
#[derive(Debug)]
pub struct MethodDef {
    /// Method name
    pub name: Ident,
    /// Method parameters (excluding self)
    pub params: Vec<ParamDef>,
    /// Return type
    pub return_type: ReturnType,
    /// Whether the method is async
    #[allow(dead_code)]
    pub is_async: bool,
    /// Version when method was introduced (None = version 1)
    pub since_version: Option<u32>,
    /// Deprecation information
    pub deprecation: Option<DeprecationInfo>,
    /// Required features for this method
    pub required_features: Vec<String>,
}

/// Parsed parameter definition.
#[derive(Debug)]
pub struct ParamDef {
    /// Parameter name
    pub name: Ident,
    /// Parameter type
    pub ty: Type,
    /// Whether this parameter is optional (for backward compatibility)
    pub is_optional: bool,
}

/// Parse attribute arguments.
pub fn parse_service(trait_def: &ItemTrait, attr_args: &[Meta]) -> Result<ServiceDef> {
    let name = trait_def.ident.clone();
    let mut direction = Direction::default();
    let mut version = 1u32;
    let mut min_version = 1u32;
    let mut features = Vec::new();

    // Parse attributes
    for meta in attr_args {
        match meta {
            Meta::NameValue(MetaNameValue {
                path,
                value: Expr::Lit(expr_lit),
                ..
            }) if path.is_ident("direction") => {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    direction = Direction::from_str(&lit_str.value())?;
                } else {
                    return Err(Error::new_spanned(
                        expr_lit,
                        "direction attribute must be a string literal",
                    ));
                }
            }
            Meta::NameValue(MetaNameValue {
                path,
                value: Expr::Lit(expr_lit),
                ..
            }) if path.is_ident("version") => {
                if let Lit::Int(lit_int) = &expr_lit.lit {
                    version = lit_int.base10_parse()?;
                } else {
                    return Err(Error::new_spanned(
                        expr_lit,
                        "version attribute must be an integer literal",
                    ));
                }
            }
            Meta::NameValue(MetaNameValue {
                path,
                value: Expr::Lit(expr_lit),
                ..
            }) if path.is_ident("min_version") => {
                if let Lit::Int(lit_int) = &expr_lit.lit {
                    min_version = lit_int.base10_parse()?;
                } else {
                    return Err(Error::new_spanned(
                        expr_lit,
                        "min_version attribute must be an integer literal",
                    ));
                }
            }
            Meta::List(MetaList { path, tokens, .. }) if path.is_ident("features") => {
                // Parse comma-separated string literals
                let parser = syn::punctuated::Punctuated::<Lit, syn::Token![,]>::parse_terminated;
                let literals = parser.parse2(tokens.clone())?;
                for lit in literals {
                    if let Lit::Str(lit_str) = lit {
                        features.push(lit_str.value());
                    } else {
                        return Err(Error::new_spanned(lit, "features must be string literals"));
                    }
                }
            }
            _ => {
                return Err(Error::new_spanned(
                    meta,
                    "Unknown attribute. Supported: direction, version, min_version, features",
                ));
            }
        }
    }

    // Validate version constraints
    if min_version > version {
        return Err(Error::new(
            Span::call_site(),
            format!(
                "min_version ({}) cannot be greater than version ({})",
                min_version, version
            ),
        ));
    }

    // Parse methods
    let mut methods = Vec::new();
    for item in &trait_def.items {
        if let TraitItem::Fn(method) = item {
            methods.push(parse_method(method)?);
        }
    }

    if methods.is_empty() {
        return Err(Error::new_spanned(
            trait_def,
            "Service trait must have at least one method",
        ));
    }

    Ok(ServiceDef {
        trait_def: trait_def.clone(),
        name,
        direction,
        version,
        min_version,
        features,
        methods,
    })
}

/// Parse a trait method.
fn parse_method(method: &TraitItemFn) -> Result<MethodDef> {
    let name = method.sig.ident.clone();
    let is_async = method.sig.asyncness.is_some();
    let mut since_version = None;
    let mut deprecation = None;
    let mut required_features = Vec::new();

    // Parse method attributes
    for attr in &method.attrs {
        if attr.path().is_ident("since") {
            since_version = Some(parse_since_attribute(attr)?);
        } else if attr.path().is_ident("deprecated") {
            deprecation = Some(parse_deprecated_attribute(attr)?);
        } else if attr.path().is_ident("requires_feature") {
            required_features.extend(parse_requires_feature_attribute(attr)?);
        }
    }

    // Extract parameters (skip self)
    let mut params = Vec::new();
    for arg in &method.sig.inputs {
        match arg {
            FnArg::Receiver(_) => continue, // Skip self
            FnArg::Typed(PatType { pat, ty, attrs, .. }) => {
                if let Pat::Ident(pat_ident) = &**pat {
                    // Check if parameter has #[optional] attribute
                    let is_optional = attrs.iter().any(|attr| attr.path().is_ident("optional"));

                    params.push(ParamDef {
                        name: pat_ident.ident.clone(),
                        ty: (**ty).clone(),
                        is_optional,
                    });
                } else {
                    return Err(Error::new_spanned(
                        pat,
                        "Only simple parameter names are supported",
                    ));
                }
            }
        }
    }

    // Validate return type - must be async or return impl Future
    let return_type = method.sig.output.clone();
    
    if !is_async {
        // Check if return type is impl Future
        let is_future = match &return_type {
            ReturnType::Type(_, ty) => is_impl_future(ty),
            ReturnType::Default => false,
        };
        
        if !is_future {
            return Err(Error::new_spanned(
                &method.sig,
                "Service methods must be async or return impl Future<Output = ...>",
            ));
        }
    }

    Ok(MethodDef {
        name,
        params,
        return_type,
        is_async,
        since_version,
        deprecation,
        required_features,
    })
}

/// Check if a type is `impl Future` or similar Future type.
fn is_impl_future(ty: &Type) -> bool {
    match ty {
        Type::ImplTrait(impl_trait) => {
            // Check if any bound is Future
            impl_trait.bounds.iter().any(|bound| {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    trait_bound.path.segments.iter().any(|seg| {
                        seg.ident == "Future"
                    })
                } else {
                    false
                }
            })
        }
        Type::Path(type_path) => {
            // Check for Pin<Box<dyn Future<...>>> or similar patterns
            contains_future_in_path(type_path)
        }
        _ => false,
    }
}

/// Recursively check if a type path contains Future anywhere in its structure.
fn contains_future_in_path(type_path: &syn::TypePath) -> bool {
    for segment in &type_path.path.segments {
        // Direct Future reference
        if segment.ident == "Future" {
            return true;
        }
        
        // Check generic arguments (e.g., Pin<Box<dyn Future>>)
        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
            for arg in &args.args {
                match arg {
                    syn::GenericArgument::Type(inner_ty) => {
                        if contains_future_in_type(inner_ty) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    false
}

/// Recursively check if a type contains Future.
fn contains_future_in_type(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => contains_future_in_path(type_path),
        Type::TraitObject(trait_obj) => {
            // Check dyn Future
            trait_obj.bounds.iter().any(|bound| {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    trait_bound.path.segments.iter().any(|seg| seg.ident == "Future")
                } else {
                    false
                }
            })
        }
        Type::ImplTrait(impl_trait) => {
            // Check impl Future
            impl_trait.bounds.iter().any(|bound| {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    trait_bound.path.segments.iter().any(|seg| seg.ident == "Future")
                } else {
                    false
                }
            })
        }
        _ => false,
    }
}

/// Parse #[since(version = N)] attribute.
fn parse_since_attribute(attr: &Attribute) -> Result<u32> {
    let meta = attr.meta.require_list()?;
    let nested: MetaNameValue = syn::parse2(meta.tokens.clone())?;

    if !nested.path.is_ident("version") {
        return Err(Error::new_spanned(
            &nested.path,
            "Expected 'version' in #[since(version = N)]",
        ));
    }

    if let Expr::Lit(ExprLit {
        lit: Lit::Int(lit_int),
        ..
    }) = &nested.value
    {
        lit_int.base10_parse()
    } else {
        Err(Error::new_spanned(
            &nested.value,
            "version must be an integer literal",
        ))
    }
}

/// Parse #[deprecated(...)] attribute.
fn parse_deprecated_attribute(attr: &Attribute) -> Result<DeprecationInfo> {
    let mut since = None;
    let mut note = None;
    let mut remove_in = None;

    // Handle both #[deprecated] and #[deprecated(...)]
    match &attr.meta {
        Meta::Path(_) => {
            // Just #[deprecated] with no arguments
            since = Some("unknown".to_string());
        }
        Meta::List(meta_list) => {
            let parser =
                syn::punctuated::Punctuated::<MetaNameValue, syn::Token![,]>::parse_terminated;
            let args = parser.parse2(meta_list.tokens.clone())?;

            for arg in args {
                if arg.path.is_ident("since") {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }) = &arg.value
                    {
                        since = Some(lit_str.value());
                    }
                } else if arg.path.is_ident("note") {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }) = &arg.value
                    {
                        note = Some(lit_str.value());
                    }
                } else if arg.path.is_ident("remove_in") {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }) = &arg.value
                    {
                        remove_in = Some(lit_str.value());
                    }
                }
            }
        }
        _ => {
            return Err(Error::new_spanned(
                attr,
                "Invalid deprecated attribute format",
            ));
        }
    }

    Ok(DeprecationInfo {
        since: since.unwrap_or_else(|| "unknown".to_string()),
        note,
        remove_in,
    })
}

/// Parse #[requires_feature("feature")] attribute.
fn parse_requires_feature_attribute(attr: &Attribute) -> Result<Vec<String>> {
    let meta = attr.meta.require_list()?;
    let parser = syn::punctuated::Punctuated::<Lit, syn::Token![,]>::parse_terminated;
    let literals = parser.parse2(meta.tokens.clone())?;

    let mut features = Vec::new();
    for lit in literals {
        if let Lit::Str(lit_str) = lit {
            features.push(lit_str.value());
        } else {
            return Err(Error::new_spanned(
                lit,
                "requires_feature must contain string literals",
            ));
        }
    }

    Ok(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_from_str() {
        assert_eq!(Direction::from_str("call").unwrap(), Direction::CallOnly);
        assert_eq!(
            Direction::from_str("call_only").unwrap(),
            Direction::CallOnly
        );
        assert_eq!(
            Direction::from_str("respond").unwrap(),
            Direction::RespondOnly
        );
        assert_eq!(
            Direction::from_str("bidirectional").unwrap(),
            Direction::Bidirectional
        );
        assert!(Direction::from_str("invalid").is_err());
    }

    #[test]
    fn test_direction_default() {
        assert_eq!(Direction::default(), Direction::Bidirectional);
    }
}
