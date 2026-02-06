//! Procedural macros for the bdrpc framework.
//!
//! This crate provides the `#[bdrpc::service]` attribute macro that generates
//! protocol enums, client stubs, and server dispatchers from trait definitions.
//!
//! # Example
//!
//! ```ignore
//! use bdrpc::service;
//!
//! #[service(direction = "bidirectional")]
//! trait Calculator {
//!     async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
//!     async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;
//! }
//! ```
//!
//! This will generate:
//! - A `CalculatorProtocol` enum with request/response variants
//! - A `CalculatorClient` struct for making RPC calls
//! - A `CalculatorServer` trait for implementing the service

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, ItemTrait, Meta, Token, parse_macro_input, punctuated::Punctuated};

mod generate;
mod parse;

/// The main `#[bdrpc::service]` attribute macro.
///
/// This macro transforms a trait definition into a complete RPC service with:
/// - Protocol enum for serialization
/// - Client stub for making calls
/// - Server dispatcher for handling requests
///
/// # Attributes
///
/// - `direction`: Specifies the communication direction (per ADR-008)
///   - `"call"` or `"call_only"`: Client can call, server responds
///   - `"respond"` or `"respond_only"`: Server can call, client responds
///   - `"bidirectional"`: Both sides can call and respond (default)
///
/// # Example
///
/// ```ignore
/// #[bdrpc::service(direction = "call")]
/// trait MyService {
///     async fn method(&self, arg: String) -> Result<i32, String>;
/// }
/// ```
#[proc_macro_attribute]
pub fn service(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);

    // Parse attribute arguments
    let attr_args = if attr.is_empty() {
        Vec::new()
    } else {
        match syn::parse::Parser::parse(Punctuated::<Meta, Token![,]>::parse_terminated, attr) {
            Ok(args) => args.into_iter().collect(),
            Err(err) => return err.to_compile_error().into(),
        }
    };

    // Parse the service definition
    let service = match parse::parse_service(&input, &attr_args) {
        Ok(service) => service,
        Err(err) => return err.to_compile_error().into(),
    };

    // Generate the protocol enum
    let protocol_enum = generate::generate_protocol_enum(&service);

    // Generate the client stub
    let client_stub = generate::generate_client_stub(&service);

    // Generate the server dispatcher
    let server_dispatcher = generate::generate_server_dispatcher(&service);

    // Combine all generated code
    let expanded = quote! {
        #input

        #protocol_enum

        #client_stub

        #server_dispatcher
    };

    TokenStream::from(expanded)
}

/// Internal derive macro for protocol enums (not public API).
#[proc_macro_derive(Protocol)]
pub fn derive_protocol(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // This is a placeholder for future protocol trait derivation
    let name = &input.ident;
    let expanded = quote! {
        impl bdrpc::Protocol for #name {
            // Implementation will be added in later phases
        }
    };

    TokenStream::from(expanded)
}
