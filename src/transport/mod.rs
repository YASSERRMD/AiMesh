//! AiMesh Transport Module
//!
//! QUIC-based transport layer for low-latency AI message delivery.
//! Targets 5M+ msgs/sec with <1ms P99 latency using quinn.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{ClientConfig, Endpoint, ServerConfig, Connection, RecvStream, SendStream};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::sync::RwLock;
use thiserror::Error;
use tracing::{debug, info, warn, error};
use dashmap::DashMap;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Send failed: {0}")]
    SendFailed(String),
    #[error("Receive failed: {0}")]
    ReceiveFailed(String),
    #[error("TLS error: {0}")]
    TlsError(String),
    #[error("Bind error: {0}")]
    BindError(String),
    #[error("Timeout")]
    Timeout,
    #[error("Connection closed")]
    ConnectionClosed,
}

/// Transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Bind address for the server
    pub bind_addr: String,
    /// Keep-alive interval in seconds
    pub keep_alive_secs: u64,
    /// Connection idle timeout in seconds
    pub idle_timeout_secs: u64,
    /// Max concurrent streams per connection
    pub max_concurrent_streams: u32,
    /// Flow control window size
    pub stream_window_size: u32,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".into(),
            keep_alive_secs: 30,
            idle_timeout_secs: 300,
            max_concurrent_streams: 1000,
            stream_window_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Connection pool for reusing connections
pub struct ConnectionPool {
    connections: DashMap<String, Connection>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }
    
    pub fn get(&self, addr: &str) -> Option<Connection> {
        self.connections.get(addr).map(|c| c.clone())
    }
    
    pub fn insert(&self, addr: String, conn: Connection) {
        self.connections.insert(addr, conn);
    }
    
    pub fn remove(&self, addr: &str) {
        self.connections.remove(addr);
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

/// QUIC transport layer using quinn
pub struct TransportLayer {
    config: TransportConfig,
    endpoint: Option<Endpoint>,
    connection_pool: Arc<ConnectionPool>,
    stats: Arc<RwLock<TransportStats>>,
}

#[derive(Debug, Clone, Default)]
pub struct TransportStats {
    pub active_connections: usize,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
}

impl TransportLayer {
    /// Create a new transport layer
    pub fn new(config: TransportConfig) -> Result<Self, TransportError> {
        info!(bind = %config.bind_addr, "Initializing QUIC transport layer");
        
        Ok(Self {
            config,
            endpoint: None,
            connection_pool: Arc::new(ConnectionPool::new()),
            stats: Arc::new(RwLock::new(TransportStats::default())),
        })
    }
    
    /// Generate self-signed certificate for development
    fn generate_self_signed_cert() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TransportError> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
            .map_err(|e| TransportError::TlsError(e.to_string()))?;
        
        let cert_der = CertificateDer::from(cert.cert);
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));
        
        Ok((vec![cert_der], key_der))
    }
    
    /// Create server configuration
    fn create_server_config(&self) -> Result<ServerConfig, TransportError> {
        let (certs, key) = Self::generate_self_signed_cert()?;
        
        let mut server_config = ServerConfig::with_single_cert(certs, key)
            .map_err(|e| TransportError::TlsError(e.to_string()))?;
        
        let transport_config = Arc::get_mut(&mut server_config.transport)
            .expect("transport config");
        transport_config.max_idle_timeout(Some(
            Duration::from_secs(self.config.idle_timeout_secs).try_into().unwrap()
        ));
        transport_config.keep_alive_interval(Some(
            Duration::from_secs(self.config.keep_alive_secs)
        ));
        transport_config.max_concurrent_uni_streams(self.config.max_concurrent_streams.into());
        transport_config.max_concurrent_bidi_streams(self.config.max_concurrent_streams.into());
        
        Ok(server_config)
    }
    
    /// Create client configuration (skip certificate verification for dev)
    fn create_client_config() -> Result<ClientConfig, TransportError> {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();
        
        Ok(ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .map_err(|e| TransportError::TlsError(e.to_string()))?
        )))
    }
    
    /// Start listening for incoming connections
    pub async fn listen(&mut self) -> Result<(), TransportError> {
        let addr: SocketAddr = self.config.bind_addr.parse()
            .map_err(|e| TransportError::BindError(format!("Invalid address: {}", e)))?;
        
        let server_config = self.create_server_config()?;
        
        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| TransportError::BindError(e.to_string()))?;
        
        info!(addr = %addr, "QUIC server listening");
        
        self.endpoint = Some(endpoint);
        Ok(())
    }
    
    /// Accept incoming connections (call in a loop)
    pub async fn accept(&self) -> Result<Connection, TransportError> {
        let endpoint = self.endpoint.as_ref()
            .ok_or_else(|| TransportError::ConnectionFailed("Server not started".into()))?;
        
        let incoming = endpoint.accept().await
            .ok_or_else(|| TransportError::ConnectionClosed)?;
        
        let connection = incoming.await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        
        let remote = connection.remote_address();
        info!(remote = %remote, "Accepted connection");
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.active_connections += 1;
        }
        
        Ok(connection)
    }
    
    /// Connect to a remote endpoint
    pub async fn connect(&self, addr: &str) -> Result<Connection, TransportError> {
        // Check connection pool first
        if let Some(conn) = self.connection_pool.get(addr) {
            if conn.close_reason().is_none() {
                return Ok(conn);
            }
            self.connection_pool.remove(addr);
        }
        
        let socket_addr: SocketAddr = addr.parse()
            .map_err(|e| TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;
        
        // Create client endpoint
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| TransportError::BindError(e.to_string()))?;
        
        endpoint.set_default_client_config(Self::create_client_config()?);
        
        // Connect
        let connection = endpoint.connect(socket_addr, "localhost")
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        
        info!(addr = %addr, "Connected to remote");
        
        // Store in pool
        self.connection_pool.insert(addr.to_string(), connection.clone());
        
        Ok(connection)
    }
    
    /// Send data and receive response
    pub async fn send(&self, addr: &str, data: Vec<u8>) -> Result<Vec<u8>, TransportError> {
        let connection = self.connect(addr).await?;
        
        // Open bidirectional stream
        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        
        // Send data with length prefix
        let len = (data.len() as u32).to_be_bytes();
        send.write_all(&len).await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        send.write_all(&data).await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        send.finish()
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.bytes_sent += data.len() as u64 + 4;
            stats.messages_sent += 1;
        }
        
        // Read response
        let response = self.read_message(&mut recv).await?;
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.bytes_received += response.len() as u64 + 4;
            stats.messages_received += 1;
        }
        
        Ok(response)
    }
    
    /// Read a length-prefixed message from a stream
    pub async fn read_message(&self, recv: &mut RecvStream) -> Result<Vec<u8>, TransportError> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        
        // Read data
        let mut data = vec![0u8; len];
        recv.read_exact(&mut data).await
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?;
        
        Ok(data)
    }
    
    /// Write a length-prefixed message to a stream
    pub async fn write_message(&self, send: &mut SendStream, data: &[u8]) -> Result<(), TransportError> {
        let len = (data.len() as u32).to_be_bytes();
        send.write_all(&len).await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        send.write_all(data).await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        Ok(())
    }
    
    /// Get transport statistics
    pub async fn stats(&self) -> TransportStats {
        self.stats.read().await.clone()
    }
    
    /// Close all connections
    pub fn close(&self) {
        if let Some(endpoint) = &self.endpoint {
            endpoint.close(0u32.into(), b"shutdown");
        }
    }
}

/// Skip server certificate verification (for development only)
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
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
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_transport_creation() {
        let config = TransportConfig::default();
        let transport = TransportLayer::new(config);
        assert!(transport.is_ok());
    }
    
    #[tokio::test]
    async fn test_cert_generation() {
        let result = TransportLayer::generate_self_signed_cert();
        assert!(result.is_ok());
    }
}
