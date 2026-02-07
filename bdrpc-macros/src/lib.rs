//! Procedural macros for the bdrpc framework.
//!
//! This crate provides the `#[bdrpc::service]` attribute macro that generates
//! protocol enums, client stubs, and server dispatchers from trait definitions.
//!
//! # Example
//!
//! ```ignore
//! use bdrpc::service;
//! use std::future::Future;
//!
//! #[service(direction = "bidirectional")]
//! trait Calculator {
//!     // Async methods are supported
//!     async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
//!
//!     // Methods returning impl Future are also supported
//!     fn subtract(&self, a: i32, b: i32) -> impl Future<Output = Result<i32, String>> + Send;
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
/// # Method Signatures
///
/// Service methods can be defined in two ways:
///
/// 1. **Async methods** (recommended for most cases):
///    ```ignore
///    async fn method(&self, arg: String) -> Result<i32, String>;
///    ```
///
/// 2. **Manual Future implementation** (for advanced use cases):
///    ```ignore
///    fn method(&self, arg: String) -> impl Future<Output = Result<i32, String>> + Send;
///    ```
///
/// Both styles are fully supported and can be mixed within the same service trait.
/// The macro will extract the `Output` type from `impl Future` and generate the
/// appropriate protocol variants.
///
/// # Example
///
/// ```ignore
/// use std::future::Future;
///
/// #[bdrpc::service(direction = "call")]
/// trait MyService {
///     // Async method
///     async fn async_method(&self, arg: String) -> Result<i32, String>;
///
///     // Manual Future method
///     fn future_method(&self, value: i32) -> impl Future<Output = Result<i32, String>> + Send;
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
