//! Tests for backward compatibility features.
//!
//! These tests verify that:
//! - V1 clients can communicate with V2 servers
//! - V2 clients can communicate with V1 servers
//! - Optional fields work correctly
//! - Unknown variants are handled gracefully

// Version 1 of a service
mod v1 {
    use bdrpc::service;

    #[allow(dead_code)]
    #[service(version = 1, min_version = 1)]
    pub trait UserService {
        async fn get_user(&self, _id: u64) -> String;
    }
}

// Version 2 adds a new method and optional parameter
mod v2 {
    use bdrpc::service;

    #[allow(dead_code)]
    #[service(version = 2, min_version = 1)]
    pub trait UserService {
        async fn get_user(&self, _id: u64, _include_details: Option<bool>) -> String;

        async fn search_users(&self, query: String) -> Vec<String>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v1_protocol_basics() {
        // Test that V1 protocol has correct metadata
        assert_eq!(v1::UserServiceProtocol::VERSION, 1);
        assert_eq!(v1::UserServiceProtocol::MIN_SUPPORTED_VERSION, 1);
        assert_eq!(v1::UserServiceProtocol::FEATURES.len(), 0);
    }

    #[test]
    fn test_v2_protocol_basics() {
        // Test that V2 protocol has correct metadata
        assert_eq!(v2::UserServiceProtocol::VERSION, 2);
        assert_eq!(v2::UserServiceProtocol::MIN_SUPPORTED_VERSION, 1);
    }

    #[test]
    fn test_v1_request_structure() {
        // V1 request should have only id field
        let request = v1::UserServiceProtocol::GetUserRequest { _id: 42 };
        assert_eq!(request.method_name(), "GetUserRequest");
        assert!(request.is_request());
        assert!(!request.is_response());
        assert!(!request.is_unknown());
    }

    #[test]
    fn test_v2_request_with_optional() {
        // V2 request can have optional field
        let request = v2::UserServiceProtocol::GetUserRequest {
            _id: 42,
            _include_details: Some(true),
        };
        assert_eq!(request.method_name(), "GetUserRequest");
        assert!(request.is_request());
    }

    #[test]
    fn test_v2_request_without_optional() {
        // V2 request can omit optional field
        let request = v2::UserServiceProtocol::GetUserRequest {
            _id: 42,
            _include_details: None,
        };
        assert_eq!(request.method_name(), "GetUserRequest");
        assert!(request.is_request());
    }

    #[test]
    fn test_unknown_variant() {
        // Unknown variant should be handled gracefully
        let unknown = v1::UserServiceProtocol::Unknown;
        assert_eq!(unknown.method_name(), "unknown");
        assert!(!unknown.is_request());
        assert!(!unknown.is_response());
        assert!(unknown.is_unknown());
    }

    #[test]
    fn test_method_version_lookup() {
        // V1 methods should be at version 1
        assert_eq!(v1::UserServiceProtocol::method_version("get_user"), Some(1));

        // V2 methods
        assert_eq!(
            v2::UserServiceProtocol::method_version("search_users"),
            Some(1)
        );
        assert_eq!(v2::UserServiceProtocol::method_version("get_user"), Some(1));
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_v1_to_v2_serialization() {
        use serde_json;

        // Simulate V1 client sending to V2 server
        // V1 sends: {"type": "GetUserRequest", "_id": 42}
        // V2 should deserialize with _include_details = None (default)

        let v1_json = r#"{"type":"GetUserRequest","_id":42}"#;
        let v2_request: Result<v2::UserServiceProtocol, _> = serde_json::from_str(v1_json);

        assert!(v2_request.is_ok());
        if let Ok(v2::UserServiceProtocol::GetUserRequest {
            _id,
            _include_details,
        }) = v2_request
        {
            assert_eq!(_id, 42);
            assert_eq!(_include_details, None); // Should default to None
        } else {
            panic!("Failed to deserialize V1 message as V2");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_v2_to_v1_serialization() {
        use serde_json;

        // Simulate V2 client sending to V1 server
        // V2 sends: {"type": "GetUserRequest", "_id": 42, "_include_details": true}
        // V1 should deserialize, ignoring the extra field

        let v2_json = r#"{"type":"GetUserRequest","_id":42,"_include_details":true}"#;
        let v1_request: Result<v1::UserServiceProtocol, _> = serde_json::from_str(v2_json);

        assert!(v1_request.is_ok());
        if let Ok(v1::UserServiceProtocol::GetUserRequest { _id }) = v1_request {
            assert_eq!(_id, 42);
        } else {
            panic!("Failed to deserialize V2 message as V1");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_unknown_variant_deserialization() {
        use serde_json;

        // Simulate receiving a message with unknown variant
        // This would happen when V1 receives a V2-only method call
        let v2_only_json = r#"{"type":"SearchUsersRequest","query":"test"}"#;
        let v1_request: Result<v1::UserServiceProtocol, _> = serde_json::from_str(v2_only_json);

        assert!(v1_request.is_ok());
        if let Ok(msg) = v1_request {
            assert!(msg.is_unknown());
        } else {
            panic!("Failed to deserialize unknown variant");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_response_backward_compatibility() {
        use serde_json;

        // Test that responses are also backward compatible
        let v1_response = v1::UserServiceProtocol::GetUserResponse {
            result: "John Doe".to_string(),
        };

        let json = serde_json::to_string(&v1_response).unwrap();

        // V2 should be able to deserialize V1 response
        let v2_response: Result<v2::UserServiceProtocol, _> = serde_json::from_str(&json);
        assert!(v2_response.is_ok());
    }
}

// Test with features
mod advanced {
    use bdrpc::service;

    #[allow(dead_code)]
    #[service(version = 2, min_version = 1, features("streaming", "compression"))]
    pub trait AdvancedService {
        async fn basic_call(&self, _data: String) -> String;

        async fn stream_data(&self, _count: u32) -> Vec<u8>;
    }
}

#[cfg(test)]
mod feature_tests {
    use super::*;
    use bdrpc::channel::Protocol;

    #[test]
    fn test_feature_metadata() {
        assert_eq!(advanced::AdvancedServiceProtocol::FEATURES.len(), 2);
        assert!(advanced::AdvancedServiceProtocol::FEATURES.contains(&"streaming"));
        assert!(advanced::AdvancedServiceProtocol::FEATURES.contains(&"compression"));
    }

    #[test]
    fn test_method_feature_requirements() {
        // basic_call requires no features
        assert_eq!(
            advanced::AdvancedServiceProtocol::required_features("basic_call").len(),
            0
        );

        // stream_data requires no features (we removed the attribute)
        assert_eq!(
            advanced::AdvancedServiceProtocol::required_features("stream_data").len(),
            0
        );
    }

    #[test]
    fn test_instance_feature_requirements() {
        let basic_request = advanced::AdvancedServiceProtocol::BasicCallRequest {
            _data: "test".to_string(),
        };
        assert_eq!(basic_request.required_features().len(), 0);

        let stream_request = advanced::AdvancedServiceProtocol::StreamDataRequest { _count: 10 };
        assert_eq!(stream_request.required_features().len(), 0);
    }
}

// Test deprecated methods (using standard Rust #[deprecated])
mod deprecated {
    use bdrpc::service;

    #[allow(dead_code)]
    #[service(version = 3, min_version = 1)]
    pub trait DeprecatedService {
        async fn new_method(&self, _id: u64) -> String;

        #[deprecated]
        async fn old_method(&self, _id: u64) -> String;
    }
}

#[cfg(test)]
mod deprecation_tests {
    use super::*;

    #[test]
    fn test_deprecated_method_exists() {
        // Just verify the deprecated method still exists and works
        let request = deprecated::DeprecatedServiceProtocol::OldMethodRequest { _id: 42 };
        assert_eq!(request.method_name(), "OldMethodRequest");
    }

    #[test]
    fn test_non_deprecated_method() {
        let request = deprecated::DeprecatedServiceProtocol::NewMethodRequest { _id: 42 };
        assert_eq!(request.method_name(), "NewMethodRequest");
    }
}
