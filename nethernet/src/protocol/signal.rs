use crate::error::{NethernetError, SignalErrorCode};
use std::fmt;
use std::str::FromStr;

/// Signal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    /// CONNECTREQUEST - Connection request (offer)
    Offer,
    /// CONNECTRESPONSE - Connection response (answer)
    Answer,
    /// CANDIDATEADD - ICE candidate addition
    Candidate,
    /// CONNECTERROR - Connection error
    Error,
}

impl SignalType {
    /// Get the protocol wire string corresponding to the signal type.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::SignalType;
    ///
    /// assert_eq!(SignalType::Offer.as_str(), "CONNECTREQUEST");
    /// assert_eq!(SignalType::Answer.as_str(), "CONNECTRESPONSE");
    /// assert_eq!(SignalType::Candidate.as_str(), "CANDIDATEADD");
    /// assert_eq!(SignalType::Error.as_str(), "CONNECTERROR");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            SignalType::Offer => "CONNECTREQUEST",
            SignalType::Answer => "CONNECTRESPONSE",
            SignalType::Candidate => "CANDIDATEADD",
            SignalType::Error => "CONNECTERROR",
        }
    }
}

impl FromStr for SignalType {
    type Err = NethernetError;

    /// Parses a signal type from its wire string representation.
    ///
    /// Recognizes the following exact strings: `"CONNECTREQUEST"`, `"CONNECTRESPONSE"`,
    /// `"CANDIDATEADD"`, and `"CONNECTERROR"`. For any other input, returns
    /// `NethernetError::Other` with an "Unknown signal type" message.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    ///
    /// let t = SignalType::from_str("CONNECTREQUEST").unwrap();
    /// assert_eq!(t, SignalType::Offer);
    ///
    /// assert!(SignalType::from_str("INVALID").is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CONNECTREQUEST" => Ok(SignalType::Offer),
            "CONNECTRESPONSE" => Ok(SignalType::Answer),
            "CANDIDATEADD" => Ok(SignalType::Candidate),
            "CONNECTERROR" => Ok(SignalType::Error),
            _ => Err(NethernetError::Other(format!("Unknown signal type: {}", s))),
        }
    }
}

impl fmt::Display for SignalType {
    /// Formats the signal type as its canonical protocol string.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::SignalType;
    /// assert_eq!(format!("{}", SignalType::Offer), "CONNECTREQUEST");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// NetherNet signal message
#[derive(Debug, Clone)]
pub struct Signal {
    /// Signal type
    pub signal_type: SignalType,
    /// Connection ID
    pub connection_id: u64,
    /// Signal data (SDP, ICE candidate, etc.)
    pub data: String,
    /// Network ID (receiver/sender)
    pub network_id: String,
}

impl Signal {
    /// Constructs a Signal from its components.
    ///
    /// # Examples
    ///
    /// ```
    /// let sig = Signal::new(SignalType::Offer, 42, "sdp".to_string(), "net1".to_string());
    /// assert_eq!(sig.connection_id, 42);
    /// assert_eq!(sig.network_id, "net1");
    /// assert_eq!(sig.data, "sdp");
    /// ```
    pub fn new(
        signal_type: SignalType,
        connection_id: u64,
        data: String,
        network_id: String,
    ) -> Self {
        Self {
            signal_type,
            connection_id,
            data,
            network_id,
        }
    }

    /// Constructs a Signal with type Offer (CONNECTREQUEST) using the given connection ID, SDP payload, and network ID.
    ///
    /// # Examples
    ///
    /// ```
    /// let sig = Signal::offer(42, "v=0...".to_string(), "net-1".to_string());
    /// assert_eq!(sig.signal_type, SignalType::Offer);
    /// assert_eq!(sig.connection_id, 42);
    /// assert_eq!(sig.data, "v=0...");
    /// assert_eq!(sig.network_id, "net-1");
    /// ```
    pub fn offer(connection_id: u64, sdp: String, network_id: String) -> Self {
        Self::new(SignalType::Offer, connection_id, sdp, network_id)
    }

    /// Creates a Signal with type `Answer` for the specified connection, SDP payload, and network.
    ///
    /// # Examples
    ///
    /// ```
    /// let sig = Signal::answer(42, "sdp-data".to_string(), "net-1".to_string());
    /// assert_eq!(sig.signal_type, SignalType::Answer);
    /// assert_eq!(sig.connection_id, 42);
    /// assert_eq!(sig.data, "sdp-data");
    /// assert_eq!(sig.network_id, "net-1");
    /// ```
    pub fn answer(connection_id: u64, sdp: String, network_id: String) -> Self {
        Self::new(SignalType::Answer, connection_id, sdp, network_id)
    }

    /// Constructs a `Candidate` signal for the given connection and network.
    ///
    /// # Examples
    ///
    /// ```
    /// let sig = Signal::candidate(42, "candidate-data".to_string(), "net-1".to_string());
    /// assert!(matches!(sig.signal_type, SignalType::Candidate));
    /// assert_eq!(sig.connection_id, 42);
    /// assert_eq!(sig.data, "candidate-data");
    /// assert_eq!(sig.network_id, "net-1");
    /// ```
    pub fn candidate(connection_id: u64, candidate: String, network_id: String) -> Self {
        Self::new(SignalType::Candidate, connection_id, candidate, network_id)
    }

    /// Create a `Signal` representing a connection error.
    ///
    /// The signal's `signal_type` is `SignalType::Error` and the `data` field
    /// contains the numeric `SignalErrorCode` encoded as a decimal string.
    ///
    /// # Examples
    ///
    /// ```
    /// let sig = Signal::error(42, SignalErrorCode::ConnectionFailed, "net1".to_string());
    /// assert_eq!(sig.signal_type, SignalType::Error);
    /// assert_eq!(sig.connection_id, 42);
    /// assert_eq!(sig.network_id, "net1");
    /// ```
    pub fn error(connection_id: u64, error_code: SignalErrorCode, network_id: String) -> Self {
        Self::new(
            SignalType::Error,
            connection_id,
            (error_code as u32).to_string(),
            network_id,
        )
    }

    /// Parses a Signal from a space-separated string and assigns the provided network ID.
    ///
    /// The input string must contain exactly three space-separated tokens in the form:
    /// `TYPE CONNECTION_ID DATA`
    /// - `TYPE` is the signal type token (e.g., `CONNECTREQUEST`, `CONNECTRESPONSE`, `CANDIDATEADD`, `CONNECTERROR`).
    /// - `CONNECTION_ID` is a base-10 unsigned integer.
    /// - `DATA` is the remaining token (signal payload such as SDP or ICE candidate).
    ///
    /// Returns `Err(NethernetError::Other)` when the input does not contain three parts or when the connection
    /// ID cannot be parsed as a `u64`. Parsing of the signal type also returns a `NethernetError` on unknown tokens.
    ///
    /// # Examples
    ///
    /// ```
    /// let s = "CONNECTREQUEST 1234 v=0...".to_string();
    /// let signal = Signal::from_string(&s, "net-1".to_string()).unwrap();
    /// assert_eq!(signal.signal_type.as_str(), "CONNECTREQUEST");
    /// assert_eq!(signal.connection_id, 1234);
    /// assert_eq!(signal.data, "v=0...");
    /// assert_eq!(signal.network_id, "net-1");
    /// ```
    pub fn from_string(s: &str, network_id: String) -> Result<Self, NethernetError> {
        let parts: Vec<&str> = s.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err(NethernetError::Other(format!(
                "Invalid signal format: expected 3 parts, got {}",
                parts.len()
            )));
        }

        let signal_type = SignalType::from_str(parts[0])?;
        let connection_id = parts[1]
            .parse::<u64>()
            .map_err(|e| NethernetError::Other(format!("Failed to parse connection ID: {}", e)))?;
        let data = parts[2].to_string();

        Ok(Self {
            signal_type,
            connection_id,
            data,
            network_id,
        })
    }
}

impl fmt::Display for Signal {
    /// Formats the signal as "TYPE CONNECTION_ID DATA".
    ///
    /// The output uses the signal type's canonical string (e.g., `CONNECTREQUEST`), followed by
    /// the connection ID and the signal data, separated by single spaces.
    ///
    /// # Examples
    ///
    /// ```
    /// let sig = Signal::offer(42, "sdp".to_string(), "net1".to_string());
    /// assert_eq!(format!("{}", sig), "CONNECTREQUEST 42 sdp");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.signal_type.as_str(),
            self.connection_id,
            self.data
        )
    }
}
