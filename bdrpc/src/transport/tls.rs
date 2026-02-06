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

//! TLS transport implementation for secure communication.
//!
//! This module provides TLS/SSL encryption for BDRPC transports using `tokio-rustls`.
//! It supports both client and server modes with configurable certificate handling.
//!
//! # Features
//!
//! - Client and server TLS modes
//! - Custom certificate and key configuration
//! - Self-signed certificate support for testing
//! - Wraps any existing transport with TLS encryption
//!
//! # Examples
//!
//! ## Client Mode
//!
//! ```rust,no_run
//! # #[cfg(feature = "tls")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, TlsTransport, TlsConfig};
//!
//! // Connect with TLS
//! let tcp = TcpTransport::connect("example.com:443").await?;
//! let config = TlsConfig::client_default("example.com")?;
//! let tls_transport = TlsTransport::connect(tcp, config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Server Mode
//!
//! ```rust,ignore
//! # #[cfg(feature = "tls")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, TlsTransport, TlsConfig};
//!
//! // Accept with TLS
//! let tcp = TcpTransport::connect("127.0.0.1:8443").await?; // In real code, use accept()
//! let config = TlsConfig::server_from_pem(
//!     include_bytes!("cert.pem"),
//!     include_bytes!("key.pem"),
//! )?;
//! let tls_transport = TlsTransport::accept(tcp, config).await?;
//! # Ok(())
//! # }
//! ```

use crate::transport::{Transport, TransportError, TransportMetadata};
use rustls::pki_types::{CertificateDer, ServerName};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

/// TLS configuration for client and server modes.
///
/// This type encapsulates the TLS configuration needed to establish
/// secure connections. It supports both client and server modes.
#[derive(Clone)]
pub enum TlsConfig {
    /// Client configuration with server name for SNI
    Client {
        /// TLS connector
        connector: Arc<TlsConnector>,
        /// Server name for SNI
        server_name: ServerName<'static>,
    },
    /// Server configuration with certificates
    Server {
        /// TLS acceptor
        acceptor: Arc<TlsAcceptor>,
    },
}

impl TlsConfig {
    /// Creates a default client configuration that uses the system's root certificates.
    ///
    /// # Arguments
    ///
    /// * `server_name` - The server name for SNI (Server Name Indication)
    ///
    /// # Errors
    ///
    /// Returns an error if the server name is invalid or the root certificates
    /// cannot be loaded.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "tls")]
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::TlsConfig;
    ///
    /// let config = TlsConfig::client_default("example.com")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn client_default(server_name: &str) -> Result<Self, TransportError> {
        let mut root_store = rustls::RootCertStore::empty();

        // Add system root certificates
        for cert in rustls_native_certs::load_native_certs().map_err(|e| TransportError::Io {
            source: io::Error::other(e),
        })? {
            root_store.add(cert).map_err(|e| TransportError::Io {
                source: io::Error::other(e),
            })?;
        }

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from(server_name.to_string())
            .map_err(|e| TransportError::Io {
                source: io::Error::new(io::ErrorKind::InvalidInput, e),
            })?
            .to_owned();

        Ok(Self::Client {
            connector: Arc::new(connector),
            server_name,
        })
    }

    /// Creates a client configuration that accepts any certificate (insecure).
    ///
    /// **WARNING**: This configuration disables certificate verification and should
    /// only be used for testing or development. Never use in production!
    ///
    /// # Arguments
    ///
    /// * `server_name` - The server name for SNI
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "tls")]
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::TlsConfig;
    ///
    /// // Only for testing!
    /// let config = TlsConfig::client_insecure("localhost")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn client_insecure(server_name: &str) -> Result<Self, TransportError> {
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from(server_name.to_string())
            .map_err(|e| TransportError::Io {
                source: io::Error::new(io::ErrorKind::InvalidInput, e),
            })?
            .to_owned();

        Ok(Self::Client {
            connector: Arc::new(connector),
            server_name,
        })
    }

    /// Creates a server configuration from PEM-encoded certificate and key.
    ///
    /// # Arguments
    ///
    /// * `cert_pem` - PEM-encoded certificate chain
    /// * `key_pem` - PEM-encoded private key
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate or key cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # #[cfg(feature = "tls")]
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::TlsConfig;
    ///
    /// let cert = include_bytes!("cert.pem");
    /// let key = include_bytes!("key.pem");
    /// let config = TlsConfig::server_from_pem(cert, key)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn server_from_pem(cert_pem: &[u8], key_pem: &[u8]) -> Result<Self, TransportError> {
        let certs = rustls_pemfile::certs(&mut &cert_pem[..])
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TransportError::Io {
                source: io::Error::new(io::ErrorKind::InvalidData, e),
            })?;

        let key = rustls_pemfile::private_key(&mut &key_pem[..])
            .map_err(|e| TransportError::Io {
                source: io::Error::new(io::ErrorKind::InvalidData, e),
            })?
            .ok_or_else(|| TransportError::Io {
                source: io::Error::new(io::ErrorKind::InvalidData, "no private key found"),
            })?;

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TransportError::Io {
                source: io::Error::new(io::ErrorKind::InvalidData, e),
            })?;

        let acceptor = TlsAcceptor::from(Arc::new(config));

        Ok(Self::Server {
            acceptor: Arc::new(acceptor),
        })
    }
}

impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client { server_name, .. } => f
                .debug_struct("TlsConfig::Client")
                .field("server_name", server_name)
                .finish(),
            Self::Server { .. } => f.debug_struct("TlsConfig::Server").finish(),
        }
    }
}

/// A transport wrapper that adds TLS encryption.
///
/// This type wraps any transport that implements [`Transport`] and adds
/// TLS encryption on top of it. It can operate in both client and server modes.
pub struct TlsTransport<T> {
    /// The underlying TLS stream
    stream: TlsStream<T>,
    /// Transport metadata
    metadata: TransportMetadata,
}

impl<T> TlsTransport<T>
where
    T: Transport,
{
    /// Establishes a TLS connection in client mode.
    ///
    /// This wraps an existing transport with TLS encryption, performing
    /// the TLS handshake as a client.
    ///
    /// # Arguments
    ///
    /// * `transport` - The underlying transport to wrap
    /// * `config` - TLS configuration (must be client mode)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The config is not in client mode
    /// - The TLS handshake fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # #[cfg(feature = "tls")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::{TcpTransport, TlsTransport, TlsConfig};
    ///
    /// let tcp = TcpTransport::connect("example.com:443").await?;
    /// let config = TlsConfig::client_default("example.com")?;
    /// let tls = TlsTransport::connect(tcp, config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(transport: T, config: TlsConfig) -> Result<Self, TransportError> {
        let (connector, server_name) = match config {
            TlsConfig::Client {
                connector,
                server_name,
            } => (connector, server_name),
            TlsConfig::Server { .. } => {
                return Err(TransportError::Io {
                    source: io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "expected client config, got server config",
                    ),
                });
            }
        };

        let base_metadata = transport.metadata().clone();
        let client_stream = connector
            .connect(server_name, transport)
            .await
            .map_err(|e| TransportError::Io {
                source: io::Error::other(e),
            })?;

        let stream = tokio_rustls::TlsStream::Client(client_stream);

        let metadata = TransportMetadata::new(base_metadata.id, "tls")
            .with_local_addr(
                base_metadata
                    .local_addr
                    .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()),
            )
            .with_peer_addr(
                base_metadata
                    .peer_addr
                    .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()),
            );

        Ok(Self { stream, metadata })
    }

    /// Accepts a TLS connection in server mode.
    ///
    /// This wraps an existing transport with TLS encryption, performing
    /// the TLS handshake as a server.
    ///
    /// # Arguments
    ///
    /// * `transport` - The underlying transport to wrap
    /// * `config` - TLS configuration (must be server mode)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The config is not in server mode
    /// - The TLS handshake fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # #[cfg(feature = "tls")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::{TcpTransport, TlsTransport, TlsConfig};
    ///
    /// let tcp = TcpTransport::connect("127.0.0.1:8443").await?;
    /// let config = TlsConfig::server_from_pem(
    ///     include_bytes!("cert.pem"),
    ///     include_bytes!("key.pem"),
    /// )?;
    /// let tls = TlsTransport::accept(tcp, config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn accept(transport: T, config: TlsConfig) -> Result<Self, TransportError> {
        let acceptor = match config {
            TlsConfig::Server { acceptor } => acceptor,
            TlsConfig::Client { .. } => {
                return Err(TransportError::Io {
                    source: io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "expected server config, got client config",
                    ),
                });
            }
        };

        let base_metadata = transport.metadata().clone();
        let server_stream = acceptor
            .accept(transport)
            .await
            .map_err(|e| TransportError::Io {
                source: io::Error::other(e),
            })?;

        let stream = tokio_rustls::TlsStream::Server(server_stream);

        let metadata = TransportMetadata::new(base_metadata.id, "tls")
            .with_local_addr(
                base_metadata
                    .local_addr
                    .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()),
            )
            .with_peer_addr(
                base_metadata
                    .peer_addr
                    .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()),
            );

        Ok(Self { stream, metadata })
    }
}

impl<T> Transport for TlsTransport<T>
where
    T: Transport,
{
    fn metadata(&self) -> &TransportMetadata {
        &self.metadata
    }

    async fn shutdown(&mut self) -> Result<(), TransportError> {
        use tokio::io::AsyncWriteExt;
        self.stream.shutdown().await.map_err(TransportError::from)
    }
}

impl<T> AsyncRead for TlsTransport<T>
where
    T: Transport,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl<T> AsyncWrite for TlsTransport<T>
where
    T: Transport,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Certificate verifier that accepts any certificate (insecure).
///
/// This is used for testing and development only. Never use in production!
#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)] // May be used in future TLS tests
    use crate::transport::MemoryTransport;

    #[test]
    fn test_tls_config_debug() {
        let config = TlsConfig::client_insecure("localhost").unwrap();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("TlsConfig::Client"));
    }

    #[tokio::test]
    async fn test_client_insecure_config() {
        let config = TlsConfig::client_insecure("localhost");
        assert!(config.is_ok());
    }
}
