use thiserror::Error;

/// Errors produced by ford-core protocol and I/O layers.
#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("adapter timeout waiting for prompt")]
    Timeout,

    #[error("adapter returned error: {0}")]
    Adapter(String),

    #[error("unexpected adapter response: {0}")]
    UnexpectedResponse(String),

    #[error("ISO-TP framing error: {0}")]
    Isotp(String),

    #[error("UDS negative response: NRC=0x{nrc:02X} ({description})")]
    UdsNegative { nrc: u8, description: String },

    #[error("UDS protocol error: {0}")]
    Uds(String),

    #[error("profile error: {0}")]
    Profile(String),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("hex decode error: {0}")]
    Hex(#[from] hex::FromHexError),

    #[error("demo fixture error: {0}")]
    Fixture(String),

    #[error("not connected")]
    NotConnected,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Map UDS negative response codes to short descriptions.
pub fn nrc_description(nrc: u8) -> &'static str {
    match nrc {
        0x10 => "generalReject",
        0x11 => "serviceNotSupported",
        0x12 => "subFunctionNotSupported",
        0x13 => "incorrectMessageLengthOrInvalidFormat",
        0x14 => "responseTooLong",
        0x21 => "busyRepeatRequest",
        0x22 => "conditionsNotCorrect",
        0x24 => "requestSequenceError",
        0x25 => "noResponseFromSubnetComponent",
        0x31 => "requestOutOfRange",
        0x33 => "securityAccessDenied",
        0x35 => "invalidKey",
        0x36 => "exceedNumberOfAttempts",
        0x37 => "requiredTimeDelayNotExpired",
        0x70 => "uploadDownloadNotAccepted",
        0x71 => "transferDataSuspended",
        0x72 => "generalProgrammingFailure",
        0x73 => "wrongBlockSequenceCounter",
        0x78 => "requestCorrectlyReceivedResponsePending",
        0x7E => "subFunctionNotSupportedInActiveSession",
        0x7F => "serviceNotSupportedInActiveSession",
        _ => "unknownNrc",
    }
}
