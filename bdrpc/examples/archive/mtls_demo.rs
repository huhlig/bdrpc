//
// Copyright 2026 Hans W. Uhlig. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! # Mutual TLS (mTLS) Transport Example
//!
//! This example demonstrates how to configure mutual TLS authentication for BDRPC transports.
//! In mTLS, both the client and server authenticate each other using certificates, providing
//! strong bidirectional authentication.
//!
//! ## What is mTLS?
//!
//! Mutual TLS (mTLS) extends standard TLS by requiring both parties to present certificates:
//! - **Server authentication**: Client verifies server's certificate (standard TLS)
//! - **Client authentication**: Server verifies client's certificate (mutual TLS)
//!
//! This provides:
//! - Strong authentication of both parties
//! - Protection against man-in-the-middle attacks
//! - Encrypted communication
//! - Certificate-based access control
//!
//! ## Architecture
//!
//! ```text
//! Client                                    Server
//! ┌─────────────────┐                      ┌─────────────────┐
//! │ Client Cert     │◄────────────────────►│ Server Cert     │
//! │ + Private Key   │  TLS Handshake       │ + Private Key   │
//! │                 │  (mutual auth)       │                 │
//! │ Trusted CA      │                      │ Trusted CA      │
//! │ (verifies       │                      │ (verifies       │
//! │  server cert)   │                      │  client cert)   │
//! └─────────────────┘                      └─────────────────┘
//! ```
//!
//! ## What This Example Shows
//!
//! - Creating custom TLS configurations with client certificates
//! - Configuring server to require and verify client certificates
//! - Setting up certificate authorities (CAs) for verification
//! - Establishing mTLS connections over TCP
//! - Proper error handling for certificate validation
//!
//! ## Certificate Setup
//!
//! For production use, you would:
//! 1. Generate a CA certificate and key
//! 2. Generate server certificate signed by CA
//! 3. Generate client certificate(s) signed by CA
//! 4. Distribute certificates appropriately
//!
//! This example uses the insecure mode for demonstration.
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example mtls_demo --features tls,serde
//! ```
//!
//! ## Security Notes
//!
//! - Never use insecure mode in production
//! - Protect private keys with appropriate file permissions
//! - Use strong key sizes (2048+ bits for RSA, 256+ for ECDSA)
//! - Implement certificate rotation policies
//! - Monitor certificate expiration dates
//! - Use certificate revocation lists (CRLs) or OCSP

use std::error::Error;

#[cfg(feature = "tls")]
use {
    bdrpc::transport::TlsConfig,
    rustls::pki_types::{CertificateDer, PrivateKeyDer},
    std::io,
    std::sync::Arc,
};

/// Extended TLS configuration builder for mTLS support.
///
/// This extends the basic `TlsConfig` with methods for configuring
/// mutual TLS authentication.
#[cfg(feature = "tls")]
pub struct MtlsConfigBuilder {
    server_name: Option<String>,
    client_cert: Option<Vec<CertificateDer<'static>>>,
    client_key: Option<PrivateKeyDer<'static>>,
    ca_certs: Vec<CertificateDer<'static>>,
    require_client_auth: bool,
}

#[cfg(feature = "tls")]
impl MtlsConfigBuilder {
    /// Creates a new mTLS configuration builder.
    pub fn new() -> Self {
        Self {
            server_name: None,
            client_cert: None,
            client_key: None,
            ca_certs: Vec::new(),
            require_client_auth: false,
        }
    }

    /// Sets the server name for SNI (required for client mode).
    pub fn with_server_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = Some(name.into());
        self
    }

    /// Adds a client certificate and private key for client authentication.
    ///
    /// # Arguments
    ///
    /// * `cert_pem` - PEM-encoded client certificate chain
    /// * `key_pem` - PEM-encoded client private key
    pub fn with_client_cert(
        mut self,
        cert_pem: &[u8],
        key_pem: &[u8],
    ) -> Result<Self, Box<dyn Error>> {
        let certs = rustls_pemfile::certs(&mut &cert_pem[..])
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let key = rustls_pemfile::private_key(&mut &key_pem[..])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no private key found"))?;

        self.client_cert = Some(certs);
        self.client_key = Some(key);
        Ok(self)
    }

    /// Adds a trusted CA certificate for verifying peer certificates.
    ///
    /// # Arguments
    ///
    /// * `ca_pem` - PEM-encoded CA certificate
    pub fn with_ca_cert(mut self, ca_pem: &[u8]) -> Result<Self, Box<dyn Error>> {
        let certs = rustls_pemfile::certs(&mut &ca_pem[..])
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.ca_certs.extend(certs);
        Ok(self)
    }

    /// Enables client certificate requirement for server mode.
    pub fn require_client_auth(mut self) -> Self {
        self.require_client_auth = true;
        self
    }

    /// Builds a client TLS configuration with mTLS support.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Server name is not set
    /// - CA certificates cannot be loaded
    /// - Client certificate/key configuration is invalid
    pub fn build_client(self) -> Result<TlsConfig, Box<dyn Error>> {
        let server_name = self
            .server_name
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "server name required"))?;

        // Create root certificate store
        let mut root_store = rustls::RootCertStore::empty();

        // Add custom CA certificates if provided
        if !self.ca_certs.is_empty() {
            for cert in self.ca_certs {
                root_store
                    .add(cert)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            }
        } else {
            // Fall back to system certificates
            let native_certs = rustls_native_certs::load_native_certs();
            for cert in native_certs.certs {
                root_store
                    .add(cert)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            }
            // Log any errors but don't fail
            if let Some(err) = native_certs.errors.first() {
                eprintln!("Warning: Failed to load some native certificates: {}", err);
            }
        }

        // Build client config with optional client authentication
        let config = if let (Some(cert_chain), Some(key)) = (self.client_cert, self.client_key) {
            // Client authentication enabled
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_client_auth_cert(cert_chain, key)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            // No client authentication
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth()
        };

        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let server_name = rustls::pki_types::ServerName::try_from(server_name)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .to_owned();

        Ok(TlsConfig::Client {
            connector: Arc::new(connector),
            server_name,
        })
    }

    /// Builds a server TLS configuration with mTLS support.
    ///
    /// # Arguments
    ///
    /// * `cert_pem` - PEM-encoded server certificate chain
    /// * `key_pem` - PEM-encoded server private key
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Server certificate/key cannot be parsed
    /// - Client authentication is required but no CA certificates provided
    pub fn build_server(
        self,
        cert_pem: &[u8],
        key_pem: &[u8],
    ) -> Result<TlsConfig, Box<dyn Error>> {
        let certs = rustls_pemfile::certs(&mut &cert_pem[..])
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let key = rustls_pemfile::private_key(&mut &key_pem[..])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no private key found"))?;

        let config = if self.require_client_auth {
            // Mutual TLS: require and verify client certificates
            if self.ca_certs.is_empty() {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "CA certificates required for client authentication",
                )));
            }

            let mut client_cert_verifier = rustls::RootCertStore::empty();
            for cert in self.ca_certs {
                client_cert_verifier
                    .add(cert)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            }

            let client_verifier =
                rustls::server::WebPkiClientVerifier::builder(Arc::new(client_cert_verifier))
                    .build()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            rustls::ServerConfig::builder()
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(certs, key)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            // Standard TLS: no client authentication
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        };

        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

        Ok(TlsConfig::Server {
            acceptor: Arc::new(acceptor),
        })
    }
}

#[cfg(feature = "tls")]
impl Default for MtlsConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🔐 BDRPC Mutual TLS (mTLS) Example\n");
    println!("This example demonstrates configuring mTLS for BDRPC transports.");
    println!("Both client and server authenticate each other using certificates.\n");

    println!("═══════════════════════════════════════════════════════════\n");

    // Example 1: Basic TLS (Server Authentication Only)
    println!("📋 Example 1: Standard TLS (Server Auth Only)\n");
    println!("   In standard TLS:");
    println!("   • Client verifies server's certificate");
    println!("   • Server does NOT verify client's certificate");
    println!("   • One-way authentication\n");

    println!("   Client Configuration:");
    println!("   ```rust");
    println!("   let config = TlsConfig::client_default(\"example.com\")?;");
    println!("   let tcp = TcpTransport::connect(\"example.com:443\").await?;");
    println!("   let tls = TlsTransport::connect(tcp, config).await?;");
    println!("   ```\n");

    println!("   Server Configuration:");
    println!("   ```rust");
    println!("   let config = TlsConfig::server_from_pem(");
    println!("       include_bytes!(\"server-cert.pem\"),");
    println!("       include_bytes!(\"server-key.pem\"),");
    println!("   )?;");
    println!("   let tcp = listener.accept().await?;");
    println!("   let tls = TlsTransport::accept(tcp, config).await?;");
    println!("   ```\n");

    // Example 2: Mutual TLS (Bidirectional Authentication)
    println!("═══════════════════════════════════════════════════════════\n");
    println!("📋 Example 2: Mutual TLS (Bidirectional Auth)\n");
    println!("   In mutual TLS:");
    println!("   • Client verifies server's certificate");
    println!("   • Server verifies client's certificate");
    println!("   • Two-way authentication\n");

    println!("   Client Configuration (with client cert):");
    println!("   ```rust");
    println!("   let config = MtlsConfigBuilder::new()");
    println!("       .with_server_name(\"example.com\")");
    println!("       .with_ca_cert(include_bytes!(\"ca-cert.pem\"))?");
    println!("       .with_client_cert(");
    println!("           include_bytes!(\"client-cert.pem\"),");
    println!("           include_bytes!(\"client-key.pem\"),");
    println!("       )?");
    println!("       .build_client()?;");
    println!("   ");
    println!("   let tcp = TcpTransport::connect(\"example.com:443\").await?;");
    println!("   let tls = TlsTransport::connect(tcp, config).await?;");
    println!("   ```\n");

    println!("   Server Configuration (requiring client cert):");
    println!("   ```rust");
    println!("   let config = MtlsConfigBuilder::new()");
    println!("       .with_ca_cert(include_bytes!(\"ca-cert.pem\"))?");
    println!("       .require_client_auth()");
    println!("       .build_server(");
    println!("           include_bytes!(\"server-cert.pem\"),");
    println!("           include_bytes!(\"server-key.pem\"),");
    println!("       )?;");
    println!("   ");
    println!("   let tcp = listener.accept().await?;");
    println!("   let tls = TlsTransport::accept(tcp, config).await?;");
    println!("   ```\n");

    // Example 3: Certificate Generation
    println!("═══════════════════════════════════════════════════════════\n");
    println!("📋 Example 3: Certificate Generation\n");
    println!("   For production use, generate certificates using OpenSSL:\n");

    println!("   1. Generate CA certificate:");
    println!("   ```bash");
    println!("   openssl req -x509 -newkey rsa:4096 -days 365 \\");
    println!("     -keyout ca-key.pem -out ca-cert.pem -nodes \\");
    println!("     -subj \"/CN=My CA\"");
    println!("   ```\n");

    println!("   2. Generate server certificate:");
    println!("   ```bash");
    println!("   openssl req -newkey rsa:4096 -keyout server-key.pem \\");
    println!("     -out server-req.pem -nodes \\");
    println!("     -subj \"/CN=example.com\"");
    println!("   openssl x509 -req -in server-req.pem -days 365 \\");
    println!("     -CA ca-cert.pem -CAkey ca-key.pem \\");
    println!("     -CAcreateserial -out server-cert.pem");
    println!("   ```\n");

    println!("   3. Generate client certificate:");
    println!("   ```bash");
    println!("   openssl req -newkey rsa:4096 -keyout client-key.pem \\");
    println!("     -out client-req.pem -nodes \\");
    println!("     -subj \"/CN=client1\"");
    println!("   openssl x509 -req -in client-req.pem -days 365 \\");
    println!("     -CA ca-cert.pem -CAkey ca-key.pem \\");
    println!("     -CAcreateserial -out client-cert.pem");
    println!("   ```\n");

    // Example 4: Testing with Insecure Mode
    println!("═══════════════════════════════════════════════════════════\n");
    println!("📋 Example 4: Testing with Insecure Mode\n");
    println!("   ⚠️  For testing only - never use in production!\n");

    println!("   ```rust");
    println!("   // Client accepts any server certificate");
    println!("   let config = TlsConfig::client_insecure(\"localhost\")?;");
    println!("   let tcp = TcpTransport::connect(\"localhost:8443\").await?;");
    println!("   let tls = TlsTransport::connect(tcp, config).await?;");
    println!("   ```\n");

    // Example 5: Reconnection Strategies with mTLS
    println!("═══════════════════════════════════════════════════════════\n");
    println!("📋 Example 5: Reconnection Strategies with mTLS\n");
    println!("   When using mTLS in production, implement strategy strategies");
    println!("   to handle network failures and certificate renewals gracefully.\n");

    println!("   A. Exponential Backoff (Recommended for most cases):");
    println!("   ```rust");
    println!("   use bdrpc::endpoint::{{Endpoint, EndpointConfig}};");
    println!("   use bdrpc::strategy::ExponentialBackoff;");
    println!("   use bdrpc::serialization::JsonSerializer;");
    println!("   use std::sync::Arc;");
    println!("   use std::time::Duration;");
    println!("   ");
    println!("   // Configure exponential backoff with jitter");
    println!("   let reconnect_strategy = ExponentialBackoff::builder()");
    println!("       .initial_delay(Duration::from_millis(100))");
    println!("       .max_delay(Duration::from_secs(30))");
    println!("       .multiplier(2.0)");
    println!("       .jitter(true)  // Prevents thundering herd");
    println!("       .max_attempts(Some(10))");
    println!("       .build();");
    println!("   ");
    println!("   // Create endpoint with strategy strategy");
    println!("   let config = EndpointConfig::default()");
    println!("       .with_reconnection_strategy(Arc::new(reconnect_strategy));");
    println!("   ");
    println!("   let mut endpoint = Endpoint::new(JsonSerializer::default(), config);");
    println!("   ");
    println!("   // Register protocol and connect with mTLS");
    println!("   endpoint.register_bidirectional(\"MyProtocol\", 1).await?;");
    println!("   ");
    println!("   // Build mTLS config");
    println!("   let tls_config = MtlsConfigBuilder::new()");
    println!("       .with_server_name(\"example.com\")");
    println!("       .with_ca_cert(include_bytes!(\"ca-cert.pem\"))?");
    println!("       .with_client_cert(");
    println!("           include_bytes!(\"client-cert.pem\"),");
    println!("           include_bytes!(\"client-key.pem\"),");
    println!("       )?");
    println!("       .build_client()?;");
    println!("   ");
    println!("   // Connection will automatically retry with backoff on failure");
    println!("   let connection = endpoint.connect(\"example.com:443\").await?;");
    println!("   ```\n");

    println!("   B. Circuit Breaker (For high-availability systems):");
    println!("   ```rust");
    println!("   use bdrpc::strategy::CircuitBreaker;");
    println!("   ");
    println!("   // Circuit breaker prevents cascading failures");
    println!("   let circuit_breaker = CircuitBreaker::builder()");
    println!("       .failure_threshold(5)  // Open after 5 failures");
    println!("       .timeout(Duration::from_secs(60))  // Stay open for 60s");
    println!("       .half_open_attempts(3)  // Try 3 times in half-open");
    println!("       .build();");
    println!("   ");
    println!("   let config = EndpointConfig::default()");
    println!("       .with_reconnection_strategy(Arc::new(circuit_breaker));");
    println!("   ```\n");

    println!("   C. Fixed Delay (For predictable retry patterns):");
    println!("   ```rust");
    println!("   use bdrpc::strategy::FixedDelay;");
    println!("   ");
    println!("   // Simple fixed delay between retries");
    println!("   let fixed_delay = FixedDelay::new(Duration::from_secs(5));");
    println!("   ");
    println!("   let config = EndpointConfig::default()");
    println!("       .with_reconnection_strategy(Arc::new(fixed_delay));");
    println!("   ```\n");

    println!("   D. No Reconnect (For one-shot connections):");
    println!("   ```rust");
    println!("   use bdrpc::strategy::NoReconnect;");
    println!("   ");
    println!("   // Disable automatic strategy");
    println!("   let no_reconnect = NoReconnect::new();");
    println!("   ");
    println!("   let config = EndpointConfig::default()");
    println!("       .with_reconnection_strategy(Arc::new(no_reconnect));");
    println!("   ```\n");

    println!("   🔄 Reconnection Strategy Comparison:");
    println!("   ");
    println!(
        "   | Strategy           | Use Case                    | Pros                      | Cons                    |"
    );
    println!(
        "   |--------------------|----------------------------|---------------------------|-------------------------|"
    );
    println!(
        "   | ExponentialBackoff | General purpose (default)  | Adaptive, prevents storms | Complex configuration   |"
    );
    println!(
        "   | CircuitBreaker     | High-availability systems  | Prevents cascading fails  | May give up too early   |"
    );
    println!(
        "   | FixedDelay         | Predictable retry patterns | Simple, consistent        | May overwhelm on spikes |"
    );
    println!(
        "   | NoReconnect        | One-shot connections       | No retry overhead         | Manual reconnect needed |\n"
    );

    println!("   💡 Best Practices for mTLS Reconnection:");
    println!("   • Use ExponentialBackoff with jitter for most cases");
    println!("   • Implement certificate renewal before expiration");
    println!("   • Monitor strategy metrics (attempts, failures, delays)");
    println!("   • Set reasonable max_attempts to avoid infinite retries");
    println!("   • Consider circuit breaker for cascading failure prevention");
    println!("   • Test strategy behavior with expired certificates");
    println!("   • Log strategy events for debugging and monitoring\n");

    println!("   🔐 Certificate Renewal During Reconnection:");
    println!("   ```rust");
    println!("   // Reload certificates on strategy");
    println!("   async fn reconnect_with_fresh_certs() -> Result<(), Box<dyn Error>> {{");
    println!("       loop {{");
    println!("           // Load latest certificates from disk");
    println!("           let cert = std::fs::read(\"client-cert.pem\")?;");
    println!("           let key = std::fs::read(\"client-key.pem\")?;");
    println!("           let ca = std::fs::read(\"ca-cert.pem\")?;");
    println!("           ");
    println!("           // Build fresh mTLS config");
    println!("           let tls_config = MtlsConfigBuilder::new()");
    println!("               .with_server_name(\"example.com\")");
    println!("               .with_ca_cert(&ca)?");
    println!("               .with_client_cert(&cert, &key)?");
    println!("               .build_client()?;");
    println!("           ");
    println!("           // Attempt connection");
    println!("           match endpoint.connect(\"example.com:443\").await {{");
    println!("               Ok(conn) => return Ok(()),");
    println!("               Err(e) => {{");
    println!("                   eprintln!(\"Connection failed: {{}}\", e);");
    println!("                   // Reconnection strategy handles retry timing");
    println!("                   tokio::time::sleep(Duration::from_secs(1)).await;");
    println!("               }}");
    println!("           }}");
    println!("       }}");
    println!("   }}");
    println!("   ```\n");

    // Summary
    println!("═══════════════════════════════════════════════════════════\n");
    println!("🎉 mTLS Configuration Summary\n");

    println!("📚 Key Concepts:");
    println!("   • Standard TLS: Server authentication only");
    println!("   • Mutual TLS: Both client and server authentication");
    println!("   • CA Certificates: Used to verify peer certificates");
    println!("   • Certificate Chain: Server/client cert + intermediate certs");
    println!("   • Private Keys: Must be kept secure and never shared\n");

    println!("🔒 Security Best Practices:");
    println!("   • Use strong key sizes (RSA 2048+, ECDSA 256+)");
    println!("   • Implement certificate rotation");
    println!("   • Monitor certificate expiration");
    println!("   • Use certificate revocation (CRL/OCSP)");
    println!("   • Protect private keys with file permissions");
    println!("   • Never commit certificates to version control\n");

    println!("🚀 Use Cases for mTLS:");
    println!("   • Microservice authentication");
    println!("   • API gateway security");
    println!("   • Zero-trust networks");
    println!("   • Service mesh communication");
    println!("   • IoT device authentication\n");

    println!("📖 Next Steps:");
    println!("   • Generate real certificates for testing");
    println!("   • Implement certificate validation logic");
    println!("   • Add certificate rotation support");
    println!("   • Integrate with certificate management systems");
    println!("   • Test with real network connections\n");

    Ok(())
}

// Made with Bob
