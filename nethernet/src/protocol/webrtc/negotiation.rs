//! WebRTC negotiation messages.
//!
//! These messages are sent during WebRTC connection establishment.
//! Format: `MESSAGETYPE CONNECTIONID DATA`

use crate::error::{NethernetError, Result};
use super::error::ConnectError;

/// Message types used for WebRTC negotiation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegotiationMessage {
    /// CONNECTREQUEST - Contains SDP offer from client
    ConnectRequest {
        connection_id: u64,
        sdp_offer: String,
    },
    /// CONNECTRESPONSE - Contains SDP answer from server
    ConnectResponse {
        connection_id: u64,
        sdp_answer: String,
    },
    /// CANDIDATEADD - Contains ICE candidate
    CandidateAdd {
        connection_id: u64,
        candidate: String,
    },
    /// CONNECTERROR - Contains error code
    ConnectError {
        connection_id: u64,
        error: ConnectError,
    },
}

impl NegotiationMessage {
    /// Parses a negotiation message from string format.
    ///
    /// Format: `MESSAGETYPE CONNECTIONID DATA`
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(3, ' ').collect();
        if parts.len() < 2 {
            return Err(NethernetError::Other("invalid message format".to_string()));
        }

        let message_type = parts[0];
        let connection_id = parts[1].parse::<u64>()
            .map_err(|e| NethernetError::Other(format!("invalid connection ID: {}", e)))?;

        match message_type {
            "CONNECTREQUEST" => {
                if parts.len() < 3 {
                    return Err(NethernetError::Other("missing SDP offer".to_string()));
                }
                Ok(Self::ConnectRequest {
                    connection_id,
                    sdp_offer: parts[2].to_string(),
                })
            }
            "CONNECTRESPONSE" => {
                if parts.len() < 3 {
                    return Err(NethernetError::Other("missing SDP answer".to_string()));
                }
                Ok(Self::ConnectResponse {
                    connection_id,
                    sdp_answer: parts[2].to_string(),
                })
            }
            "CANDIDATEADD" => {
                if parts.len() < 3 {
                    return Err(NethernetError::Other("missing ICE candidate".to_string()));
                }
                Ok(Self::CandidateAdd {
                    connection_id,
                    candidate: parts[2].to_string(),
                })
            }
            "CONNECTERROR" => {
                if parts.len() < 3 {
                    return Err(NethernetError::Other("missing error code".to_string()));
                }
                let error_code = parts[2].parse::<u8>()
                    .map_err(|e| NethernetError::Other(format!("invalid error code: {}", e)))?;
                let error = ConnectError::from_code(error_code)
                    .ok_or_else(|| NethernetError::Other(format!("unknown error code: {}", error_code)))?;
                Ok(Self::ConnectError {
                    connection_id,
                    error,
                })
            }
            _ => Err(NethernetError::Other(format!("unknown message type: {}", message_type))),
        }
    }

    /// Converts the message to string format.
    pub fn to_string(&self) -> String {
        match self {
            Self::ConnectRequest { connection_id, sdp_offer } => {
                format!("CONNECTREQUEST {} {}", connection_id, sdp_offer)
            }
            Self::ConnectResponse { connection_id, sdp_answer } => {
                format!("CONNECTRESPONSE {} {}", connection_id, sdp_answer)
            }
            Self::CandidateAdd { connection_id, candidate } => {
                format!("CANDIDATEADD {} {}", connection_id, candidate)
            }
            Self::ConnectError { connection_id, error } => {
                format!("CONNECTERROR {} {}", connection_id, error.code())
            }
        }
    }

    /// Returns the connection ID of the message.
    pub fn connection_id(&self) -> u64 {
        match self {
            Self::ConnectRequest { connection_id, .. } |
            Self::ConnectResponse { connection_id, .. } |
            Self::CandidateAdd { connection_id, .. } |
            Self::ConnectError { connection_id, .. } => *connection_id,
        }
    }

    /// Returns the message type as a string.
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::ConnectRequest { .. } => "CONNECTREQUEST",
            Self::ConnectResponse { .. } => "CONNECTRESPONSE",
            Self::CandidateAdd { .. } => "CANDIDATEADD",
            Self::ConnectError { .. } => "CONNECTERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_request_roundtrip() {
        let msg = NegotiationMessage::ConnectRequest {
            connection_id: 12345,
            sdp_offer: "v=0\r\no=- 123456789 2 IN IP4 127.0.0.1".to_string(),
        };
        let s = msg.to_string();
        let parsed = NegotiationMessage::parse(&s).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_connect_response_roundtrip() {
        let msg = NegotiationMessage::ConnectResponse {
            connection_id: 67890,
            sdp_answer: "v=0\r\no=- 987654321 2 IN IP4 192.168.1.1".to_string(),
        };
        let s = msg.to_string();
        let parsed = NegotiationMessage::parse(&s).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_candidate_add_roundtrip() {
        let msg = NegotiationMessage::CandidateAdd {
            connection_id: 11111,
            candidate: "candidate:1234567890 1 udp 2130706431 192.168.1.100 54321 typ host".to_string(),
        };
        let s = msg.to_string();
        let parsed = NegotiationMessage::parse(&s).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_connect_error_roundtrip() {
        let msg = NegotiationMessage::ConnectError {
            connection_id: 99999,
            error: ConnectError::NegotiationTimeout,
        };
        let s = msg.to_string();
        let parsed = NegotiationMessage::parse(&s).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_invalid_format() {
        assert!(NegotiationMessage::parse("INVALID").is_err());
        assert!(NegotiationMessage::parse("CONNECTREQUEST").is_err());
        assert!(NegotiationMessage::parse("CONNECTREQUEST abc").is_err());
    }
}
