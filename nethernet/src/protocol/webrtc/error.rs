//! WebRTC connection error codes.
//!
//! These error codes are sent in CONNECTERROR messages during WebRTC negotiation.

use std::fmt;

/// WebRTC connection error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectError {
    None = 0x00,
    DestinationNotLoggedIn = 0x01,
    NegotiationTimeout = 0x02,
    WrongTransportVersion = 0x03,
    FailedToCreatePeerConnection = 0x04,
    ICE = 0x05,
    ConnectRequest = 0x06,
    ConnectResponse = 0x07,
    CandidateAdd = 0x08,
    InactivityTimeout = 0x09,
    FailedToCreateOffer = 0x0a,
    FailedToCreateAnswer = 0x0b,
    FailedToSetLocalDescription = 0x0c,
    FailedToSetRemoteDescription = 0x0d,
    NegotiationTimeoutWaitingForResponse = 0x0e,
    NegotiationTimeoutWaitingForAccept = 0x0f,
    IncomingConnectionIgnored = 0x10,
    SignalingParsingFailure = 0x11,
    SignalingUnknownError = 0x12,
    SignalingUnicastMessageDeliveryFailed = 0x13,
    SignalingBroadcastDeliveryFailed = 0x15,
    SignalingMessageDeliveryFailed = 0x16,
    SignalingTurnAuthFailed = 0x17,
    SignalingFallbackToBestEffortDelivery = 0x18,
    NoSignalingChannel = 0x19,
    NotLoggedIn = 0x1a,
    SignalingFailedToSend = 0x1b,
}

impl ConnectError {
    /// Converts from error code to ConnectError.
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0x00 => Some(Self::None),
            0x01 => Some(Self::DestinationNotLoggedIn),
            0x02 => Some(Self::NegotiationTimeout),
            0x03 => Some(Self::WrongTransportVersion),
            0x04 => Some(Self::FailedToCreatePeerConnection),
            0x05 => Some(Self::ICE),
            0x06 => Some(Self::ConnectRequest),
            0x07 => Some(Self::ConnectResponse),
            0x08 => Some(Self::CandidateAdd),
            0x09 => Some(Self::InactivityTimeout),
            0x0a => Some(Self::FailedToCreateOffer),
            0x0b => Some(Self::FailedToCreateAnswer),
            0x0c => Some(Self::FailedToSetLocalDescription),
            0x0d => Some(Self::FailedToSetRemoteDescription),
            0x0e => Some(Self::NegotiationTimeoutWaitingForResponse),
            0x0f => Some(Self::NegotiationTimeoutWaitingForAccept),
            0x10 => Some(Self::IncomingConnectionIgnored),
            0x11 => Some(Self::SignalingParsingFailure),
            0x12 => Some(Self::SignalingUnknownError),
            0x13 => Some(Self::SignalingUnicastMessageDeliveryFailed),
            0x15 => Some(Self::SignalingBroadcastDeliveryFailed),
            0x16 => Some(Self::SignalingMessageDeliveryFailed),
            0x17 => Some(Self::SignalingTurnAuthFailed),
            0x18 => Some(Self::SignalingFallbackToBestEffortDelivery),
            0x19 => Some(Self::NoSignalingChannel),
            0x1a => Some(Self::NotLoggedIn),
            0x1b => Some(Self::SignalingFailedToSend),
            _ => None,
        }
    }

    /// Converts to error code.
    pub fn code(&self) -> u8 {
        *self as u8
    }
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "no error"),
            Self::DestinationNotLoggedIn => write!(f, "destination not logged in"),
            Self::NegotiationTimeout => write!(f, "negotiation timeout"),
            Self::WrongTransportVersion => write!(f, "wrong transport version"),
            Self::FailedToCreatePeerConnection => write!(f, "failed to create peer connection"),
            Self::ICE => write!(f, "ICE error"),
            Self::ConnectRequest => write!(f, "connect request error"),
            Self::ConnectResponse => write!(f, "connect response error"),
            Self::CandidateAdd => write!(f, "candidate add error"),
            Self::InactivityTimeout => write!(f, "inactivity timeout"),
            Self::FailedToCreateOffer => write!(f, "failed to create offer"),
            Self::FailedToCreateAnswer => write!(f, "failed to create answer"),
            Self::FailedToSetLocalDescription => write!(f, "failed to set local description"),
            Self::FailedToSetRemoteDescription => write!(f, "failed to set remote description"),
            Self::NegotiationTimeoutWaitingForResponse => write!(f, "negotiation timeout waiting for response"),
            Self::NegotiationTimeoutWaitingForAccept => write!(f, "negotiation timeout waiting for accept"),
            Self::IncomingConnectionIgnored => write!(f, "incoming connection ignored"),
            Self::SignalingParsingFailure => write!(f, "signaling parsing failure"),
            Self::SignalingUnknownError => write!(f, "signaling unknown error"),
            Self::SignalingUnicastMessageDeliveryFailed => write!(f, "signaling unicast message delivery failed"),
            Self::SignalingBroadcastDeliveryFailed => write!(f, "signaling broadcast delivery failed"),
            Self::SignalingMessageDeliveryFailed => write!(f, "signaling message delivery failed"),
            Self::SignalingTurnAuthFailed => write!(f, "signaling TURN auth failed"),
            Self::SignalingFallbackToBestEffortDelivery => write!(f, "signaling fallback to best effort delivery"),
            Self::NoSignalingChannel => write!(f, "no signaling channel"),
            Self::NotLoggedIn => write!(f, "not logged in"),
            Self::SignalingFailedToSend => write!(f, "signaling failed to send"),
        }
    }
}

impl std::error::Error for ConnectError {}
