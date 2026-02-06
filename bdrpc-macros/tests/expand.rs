//! Tests for macro expansion.
//!
//! These tests verify that the `#[bdrpc::service]` macro generates the expected code.

use bdrpc::service;

/// A simple calculator service for testing.
#[allow(dead_code)]
#[service(direction = "bidirectional")]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;
}

#[test]
fn test_protocol_enum_generated() {
    // Verify that the protocol enum was generated
    let _request = CalculatorProtocol::AddRequest { a: 1, b: 2 };
    let _response = CalculatorProtocol::AddResponse { result: Ok(3) };
}

#[test]
fn test_protocol_methods() {
    let request = CalculatorProtocol::AddRequest { a: 1, b: 2 };
    assert!(request.is_request());
    assert!(!request.is_response());

    let response = CalculatorProtocol::AddResponse { result: Ok(3) };
    assert!(!response.is_request());
    assert!(response.is_response());
}
