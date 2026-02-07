//! Code generation for the `#[bdrpc::service]` macro.
//!
//! This module generates:
//! - Protocol enums with request/response variants
//! - Client stubs for making RPC calls
//! - Server dispatchers for handling requests

use crate::parse::{MethodDef, ServiceDef};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ReturnType, Type, TypeImplTrait, TypeParamBound};

/// Extract the actual return type from a method signature.
/// For async methods, this is the type after `->`.
/// For methods returning `impl Future<Output = T>`, this extracts `T`.
/// For methods returning `Pin<Box<dyn Future<Output = T>>>`, this extracts `T`.
fn extract_return_type(return_type: &ReturnType) -> TokenStream {
    match return_type {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => {
            // Try to extract the Output type from various Future patterns
            if let Some(output_ty) = extract_future_output(ty) {
                return output_ty;
            }
            // If not a Future type, return the type as-is
            quote! { #ty }
        }
    }
}

/// Extract the Output type from a Future type.
/// Handles: impl Future<Output = T>, Pin<Box<dyn Future<Output = T>>>, etc.
fn extract_future_output(ty: &Type) -> Option<TokenStream> {
    match ty {
        // impl Future<Output = T>
        Type::ImplTrait(TypeImplTrait { bounds, .. }) => {
            for bound in bounds {
                if let TypeParamBound::Trait(trait_bound) = bound {
                    if let Some(output) = extract_output_from_trait_bound(trait_bound) {
                        return Some(output);
                    }
                }
            }
            None
        }
        // Pin<Box<dyn Future<Output = T>>> or similar
        Type::Path(type_path) => extract_output_from_path(type_path),
        _ => None,
    }
}

/// Extract Output type from a trait bound (e.g., Future<Output = T>).
fn extract_output_from_trait_bound(trait_bound: &syn::TraitBound) -> Option<TokenStream> {
    for segment in &trait_bound.path.segments {
        if segment.ident == "Future" {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                for arg in &args.args {
                    if let syn::GenericArgument::AssocType(assoc) = arg {
                        if assoc.ident == "Output" {
                            let output_ty = &assoc.ty;
                            return Some(quote! { #output_ty });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Recursively extract Output type from a type path (e.g., Pin<Box<dyn Future<Output = T>>>).
fn extract_output_from_path(type_path: &syn::TypePath) -> Option<TokenStream> {
    for segment in &type_path.path.segments {
        // Check if this segment has generic arguments
        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
            for arg in &args.args {
                if let syn::GenericArgument::Type(inner_ty) = arg {
                    // Recursively check inner types
                    if let Some(output) = extract_future_output(inner_ty) {
                        return Some(output);
                    }
                }
            }
        }
    }
    None
}

/// Generate the protocol enum for a service.
///
/// Creates an enum with request and response variants for each method.
/// The enum derives Serialize and Deserialize for wire transmission.
pub fn generate_protocol_enum(service: &ServiceDef) -> TokenStream {
    let protocol_name = format_ident!("{}Protocol", service.name);
    let service_name = &service.name;
    let direction = format!("{:?}", service.direction);

    // Generate variants for each method
    let mut variants = Vec::new();
    let mut variant_names = Vec::new();

    for method in &service.methods {
        let method_name = &method.name;
        let request_variant = format_ident!("{}Request", capitalize(&method.name.to_string()));
        let response_variant = format_ident!("{}Response", capitalize(&method.name.to_string()));

        // Request variant with parameters
        let request_fields: Vec<_> = method
            .params
            .iter()
            .map(|param| {
                let name = &param.name;
                let ty = &param.ty;
                if param.is_optional {
                    // Add serde default for optional fields
                    quote! {
                        #[cfg_attr(feature = "serde", serde(default))]
                        #name: #ty
                    }
                } else {
                    quote! { #name: #ty }
                }
            })
            .collect();

        // Response variant with return type
        let extracted_type = extract_return_type(&method.return_type);
        let response_field = if matches!(method.return_type, ReturnType::Default) {
            quote! {}
        } else {
            quote! { result: #extracted_type }
        };

        variants.push(quote! {
            #[doc = concat!("Request to call `", stringify!(#method_name), "` method")]
            #request_variant { #(#request_fields),* }
        });
        variant_names.push((request_variant.clone(), true));

        variants.push(quote! {
            #[doc = concat!("Response from `", stringify!(#method_name), "` method")]
            #response_variant { #response_field }
        });
        variant_names.push((response_variant.clone(), false));
    }

    // Generate match arms for method_name
    let method_name_arms: Vec<_> = variant_names
        .iter()
        .map(|(name, _)| {
            let name_str = name.to_string();
            quote! {
                Self::#name { .. } => #name_str
            }
        })
        .collect();

    // Generate match arms for is_request
    let is_request_arms: Vec<_> = variant_names
        .iter()
        .filter(|(_, is_req)| *is_req)
        .map(|(name, _)| {
            quote! {
                Self::#name { .. } => true
            }
        })
        .collect();

    // Generate version metadata
    let version = service.version;
    let min_version = service.min_version;
    let features: Vec<_> = service.features.iter().collect();

    // Generate method version mapping
    let method_version_arms: Vec<_> = service
        .methods
        .iter()
        .map(|method| {
            let method_name_str = method.name.to_string();
            let since = method.since_version.unwrap_or(1);
            quote! {
                #method_name_str => Some(#since)
            }
        })
        .collect();

    // Generate unique DeprecationInfo struct name for this protocol
    let deprecation_info_name = format_ident!("{}DeprecationInfo", service_name);

    // Generate method deprecation info
    let deprecation_arms: Vec<_> = service
        .methods
        .iter()
        .filter_map(|method| {
            method.deprecation.as_ref().map(|dep| {
                let method_name_str = method.name.to_string();
                let since = &dep.since;
                let note = dep
                    .note
                    .as_ref()
                    .map(|n| quote! { Some(#n) })
                    .unwrap_or(quote! { None });
                let remove_in = dep
                    .remove_in
                    .as_ref()
                    .map(|r| quote! { Some(#r) })
                    .unwrap_or(quote! { None });
                quote! {
                    #method_name_str => Some(#deprecation_info_name {
                        since: #since,
                        note: #note,
                        remove_in: #remove_in,
                    })
                }
            })
        })
        .collect();

    // Generate required features mapping
    let required_features_arms: Vec<_> = service
        .methods
        .iter()
        .filter(|method| !method.required_features.is_empty())
        .map(|method| {
            let method_name_str = method.name.to_string();
            let features = &method.required_features;
            quote! {
                #method_name_str => &[#(#features),*]
            }
        })
        .collect();

    // Generate instance-level required features match arms
    let instance_required_features_arms: Vec<_> = service
        .methods
        .iter()
        .flat_map(|method| {
            let request_variant = format_ident!("{}Request", capitalize(&method.name.to_string()));
            let features = &method.required_features;
            if features.is_empty() {
                None
            } else {
                Some(quote! {
                    Self::#request_variant { .. } => &[#(#features),*]
                })
            }
        })
        .collect();

    quote! {
        /// Deprecation information for a method.
        #[derive(Debug, Clone)]
        pub struct #deprecation_info_name {
            /// Version when deprecated
            pub since: &'static str,
            /// Deprecation note/message
            pub note: Option<&'static str>,
            /// Version when it will be removed
            pub remove_in: Option<&'static str>,
        }

        #[doc = concat!("Protocol enum for the `", stringify!(#service_name), "` service")]
        #[doc = ""]
        #[doc = concat!("Direction: ", #direction)]
        #[doc = concat!("Version: ", #version)]
        #[doc = concat!("Min Version: ", #min_version)]
        #[doc = ""]
        #[doc = "This enum contains request and response variants for each method in the service."]
        #[doc = "It is used for serialization and deserialization of RPC messages."]
        #[derive(Debug, Clone)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(tag = "type"))]
        pub enum #protocol_name {
            #(#variants),*,
            /// Unknown variant for forward compatibility.
            /// This allows older versions to deserialize messages from newer versions
            /// without failing, even if they don't recognize all variants.
            #[cfg_attr(feature = "serde", serde(other))]
            Unknown,
        }


        impl #protocol_name {
            /// Current protocol version.
            pub const VERSION: u32 = #version;

            /// Minimum supported protocol version.
            pub const MIN_SUPPORTED_VERSION: u32 = #min_version;

            /// Optional features supported by this protocol.
            pub const FEATURES: &'static [&'static str] = &[#(#features),*];

            /// Returns the name of the method this message is for.
            /// Returns "unknown" for unrecognized variants.
            pub fn method_name(&self) -> &'static str {
                match self {
                    #(#method_name_arms),*,
                    Self::Unknown => "unknown",
                }
            }

            /// Returns true if this is a request message.
            pub fn is_request(&self) -> bool {
                match self {
                    #(#is_request_arms,)*
                    _ => false,
                }
            }

            /// Returns true if this is a response message.
            pub fn is_response(&self) -> bool {
                !self.is_request() && !self.is_unknown()
            }

            /// Returns true if this is an unknown/unrecognized variant.
            /// This can happen when deserializing messages from a newer protocol version.
            pub fn is_unknown(&self) -> bool {
                matches!(self, Self::Unknown)
            }

            /// Returns the version when a method was introduced.
            /// Returns None if the method name is not recognized.
            pub fn method_version(method: &str) -> Option<u32> {
                match method {
                    #(#method_version_arms,)*
                    _ => None,
                }
            }

            /// Returns deprecation information for a method, if deprecated.
            pub fn deprecation_info(method: &str) -> Option<#deprecation_info_name> {
                match method {
                    #(#deprecation_arms,)*
                    _ => None,
                }
            }

            /// Returns the features required by a method (static lookup by name).
            pub fn required_features(method: &str) -> &'static [&'static str] {
                match method {
                    #(#required_features_arms,)*
                    _ => &[],
                }
            }
        }

        impl bdrpc::channel::Protocol for #protocol_name {
            fn method_name(&self) -> &'static str {
                self.method_name()
            }

            fn is_request(&self) -> bool {
                self.is_request()
            }

            fn required_features(&self) -> &'static [&'static str] {
                match self {
                    #(#instance_required_features_arms,)*
                    _ => &[],
                }
            }
        }
    }
}

/// Generate the client stub for a service.
///
/// Creates a struct that implements the service trait and sends RPC calls.
pub fn generate_client_stub(service: &ServiceDef) -> TokenStream {
    let client_name = format_ident!("{}Client", service.name);
    let service_name = &service.name;
    let protocol_name = format_ident!("{}Protocol", service.name);

    // Generate method implementations
    let methods: Vec<_> = service
        .methods
        .iter()
        .map(|method| generate_client_method(method, &protocol_name))
        .collect();

    quote! {
        #[doc = concat!("Client stub for the `", stringify!(#service_name), "` service")]
        #[doc = ""]
        #[doc = "This struct provides methods to make RPC calls to a remote service."]
        #[doc = "It handles serialization, transport, and deserialization automatically."]
        #[derive(Debug, Clone)]
        pub struct #client_name {
            /// Channel sender for sending requests to the remote service.
            sender: ::bdrpc::channel::ChannelSender<#protocol_name>,
            /// Channel receiver for receiving responses from the remote service.
            receiver: std::sync::Arc<tokio::sync::Mutex<::bdrpc::channel::ChannelReceiver<#protocol_name>>>,
        }

        impl #client_name {
            /// Create a new client instance with the given channel.
            ///
            /// # Arguments
            ///
            /// * `sender` - Channel sender for sending requests
            /// * `receiver` - Channel receiver for receiving responses
            ///
            /// # Example
            ///
            /// ```rust,no_run
            /// # use bdrpc::channel::Channel;
            /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
            /// // Create a channel pair (typically done via endpoint)
            /// // let (sender, receiver) = endpoint.create_channel().await?;
            /// // let client = MyServiceClient::new(sender, receiver);
            /// # Ok(())
            /// # }
            /// ```
            pub fn new(
                sender: ::bdrpc::channel::ChannelSender<#protocol_name>,
                receiver: ::bdrpc::channel::ChannelReceiver<#protocol_name>,
            ) -> Self {
                Self {
                    sender,
                    receiver: std::sync::Arc::new(tokio::sync::Mutex::new(receiver)),
                }
            }

            #(#methods)*
        }
    }
}

/// Generate a single client method.
fn generate_client_method(method: &MethodDef, protocol_name: &Ident) -> TokenStream {
    let method_name = &method.name;
    let request_variant = format_ident!("{}Request", capitalize(&method.name.to_string()));

    // Method parameters
    let params: Vec<_> = method
        .params
        .iter()
        .map(|param| {
            let name = &param.name;
            let ty = &param.ty;
            quote! { #name: #ty }
        })
        .collect();

    // Parameter names for the request
    let param_names: Vec<_> = method.params.iter().map(|p| &p.name).collect();

    // Return type - extract from Future if needed
    let return_type = extract_return_type(&method.return_type);

    let response_variant = format_ident!("{}Response", capitalize(&method.name.to_string()));

    quote! {
        #[doc = concat!("Call the `", stringify!(#method_name), "` method on the remote service")]
        #[doc = ""]
        #[doc = "Sends a request to the remote service and waits for the response."]
        #[doc = ""]
        #[doc = "# Errors"]
        #[doc = ""]
        #[doc = "Returns an error if:"]
        #[doc = "- The channel is closed"]
        #[doc = "- Sending the request fails"]
        #[doc = "- Receiving the response fails"]
        #[doc = "- The response is not the expected variant"]
        pub async fn #method_name(&self, #(#params),*) -> Result<#return_type, ::bdrpc::channel::ChannelError> {
            // Create the request message
            let request = #protocol_name::#request_variant {
                #(#param_names),*
            };

            // Send the request through the channel
            self.sender.send(request).await?;

            // Wait for the response
            let mut receiver = self.receiver.lock().await;
            let response = receiver.recv().await.ok_or_else(|| {
                ::bdrpc::channel::ChannelError::Closed {
                    channel_id: self.sender.id(),
                }
            })?;

            // Extract the result from the response variant
            match response {
                #protocol_name::#response_variant { result } => Ok(result),
                _ => Err(::bdrpc::channel::ChannelError::Internal {
                    message: format!(
                        "Protocol violation on channel {}: Expected {} response, got different variant",
                        self.sender.id(),
                        stringify!(#response_variant)
                    ),
                }),
            }
        }
    }
}

/// Generate the server dispatcher for a service.
///
/// Creates a trait and dispatcher that routes incoming messages to trait methods.
pub fn generate_server_dispatcher(service: &ServiceDef) -> TokenStream {
    let server_name = format_ident!("{}Server", service.name);
    let dispatcher_name = format_ident!("{}Dispatcher", service.name);
    let service_name = &service.name;
    let protocol_name = format_ident!("{}Protocol", service.name);

    // Generate trait methods - preserve async vs impl Future signature
    let trait_methods: Vec<_> = service
        .methods
        .iter()
        .map(|method| {
            let method_name = &method.name;
            let params: Vec<_> = method
                .params
                .iter()
                .map(|param| {
                    let name = &param.name;
                    let ty = &param.ty;
                    quote! { #name: #ty }
                })
                .collect();

            // If the method is async, generate async fn
            // If it returns impl Future, preserve that signature
            if method.is_async {
                let return_type = extract_return_type(&method.return_type);
                quote! {
                    async fn #method_name(&self, #(#params),*) -> #return_type;
                }
            } else {
                // Preserve the original return type for impl Future methods
                let return_type = &method.return_type;
                quote! {
                    fn #method_name(&self, #(#params),*) #return_type;
                }
            }
        })
        .collect();

    // Generate dispatch match arms
    let dispatch_arms: Vec<_> = service
        .methods
        .iter()
        .map(|method| generate_dispatch_arm(method, &protocol_name))
        .collect();

    quote! {
        #[doc = concat!("Server trait for the `", stringify!(#service_name), "` service")]
        #[doc = ""]
        #[doc = "Implement this trait to provide the service functionality."]
        #[doc = "The dispatcher will route incoming RPC calls to these methods."]
        #[async_trait::async_trait]
        pub trait #server_name: Send + Sync {
            #(#trait_methods)*
        }

        #[doc = concat!("Dispatcher for the `", stringify!(#service_name), "` service")]
        #[doc = ""]
        #[doc = "Routes incoming protocol messages to the appropriate service methods."]
        pub struct #dispatcher_name<T: #server_name> {
            service: std::sync::Arc<T>,
        }

        impl<T: #server_name> #dispatcher_name<T> {
            /// Create a new dispatcher with the given service implementation.
            pub fn new(service: T) -> Self {
                Self {
                    service: std::sync::Arc::new(service)
                }
            }

            /// Dispatch an incoming message to the appropriate handler.
            ///
            /// # Errors
            ///
            /// Returns an error response if the service method fails.
            pub async fn dispatch(&self, message: #protocol_name) -> #protocol_name {
                match message {
                    #(#dispatch_arms)*
                    // Response variants and Unknown should not be dispatched
                    // They are handled by the client side
                    other => other,
                }
            }
        }
    }
}

/// Generate a dispatch match arm for a method.
fn generate_dispatch_arm(method: &MethodDef, protocol_name: &Ident) -> TokenStream {
    let method_name = &method.name;
    let request_variant = format_ident!("{}Request", capitalize(&method.name.to_string()));
    let response_variant = format_ident!("{}Response", capitalize(&method.name.to_string()));

    // Extract parameter names
    let param_names: Vec<_> = method.params.iter().map(|p| &p.name).collect();

    quote! {
        #protocol_name::#request_variant { #(#param_names),* } => {
            // Call the service method
            let result = self.service.#method_name(#(#param_names),*).await;

            // Return the response
            #protocol_name::#response_variant { result }
        }
    }
}

/// Convert snake_case to PascalCase.
fn capitalize(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("hello"), "Hello");
        assert_eq!(capitalize("world"), "World");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("a"), "A");
        assert_eq!(capitalize("get_user"), "GetUser");
        assert_eq!(capitalize("search_users"), "SearchUsers");
        assert_eq!(capitalize("stream_data"), "StreamData");
    }
}
