//! Transport-failure classification for diagnostics.
//!
//! Network failures all arrive as one opaque error type, which makes operator
//! triage hard: a DNS outage and an expired certificate look identical in the
//! logs. This module maps a failure onto a small set of operator-facing
//! categories.
//!
//! The classification is a **heuristic over the error source chain** and is meant
//! for logging only — never branch on it for control flow (retry, backoff, or
//! failover decisions must not depend on it).
//!
//! Only the resulting category is ever logged. The raw error text is not, because
//! it can carry an un-redacted URL including its query string (standards §1.3).

use crate::error::Error;

/// Best-effort classification of a transport-level failure, for diagnostics only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetErrorKind {
    /// Name resolution failed.
    Dns,
    /// TCP connect refused / unreachable / timed out.
    Tcp,
    /// TLS handshake or certificate failure.
    Tls,
    /// Client-side timeout.
    Timeout,
    /// Not classified.
    Unknown,
}

impl NetErrorKind {
    /// Short operator-facing description.
    pub fn description(self) -> &'static str {
        match self {
            Self::Dns => "DNS resolution failed — check the resolver and the API host name",
            Self::Tcp => "TCP connection failed — check network reachability and egress rules",
            Self::Tls => "TLS handshake failed — check the certificate chain and system time",
            Self::Timeout => "request timed out on the client side",
            Self::Unknown => "unclassified transport failure",
        }
    }

    /// Stable lowercase label for structured log fields.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dns => "dns",
            Self::Tcp => "tcp",
            Self::Tls => "tls",
            Self::Timeout => "timeout",
            Self::Unknown => "unknown",
        }
    }
}

/// Collect the `Display` text of an error and its whole source chain.
///
/// The result is matched against keywords and then dropped — it is never logged.
fn source_chain_text(err: &(dyn std::error::Error + 'static)) -> String {
    let mut text = err.to_string().to_lowercase();
    let mut current = err.source();
    while let Some(source) = current {
        text.push(' ');
        text.push_str(&source.to_string().to_lowercase());
        current = source.source();
    }
    text
}

/// Classify a transport failure for logging.
pub(crate) fn classify(err: &Error) -> NetErrorKind {
    if let Error::Http(http_err) = err {
        if http_err.is_timeout() {
            return NetErrorKind::Timeout;
        }
    }
    if let Error::Timeout(_) = err {
        return NetErrorKind::Timeout;
    }

    let text = source_chain_text(err);
    if text.contains("dns error")
        || text.contains("failed to lookup address")
        || text.contains("name or service not known")
        || text.contains("nodename nor servname")
        || text.contains("enotfound")
    {
        return NetErrorKind::Dns;
    }
    if text.contains("invalid peer certificate")
        || text.contains("certificate")
        || text.contains("tls")
        || text.contains("handshake")
    {
        return NetErrorKind::Tls;
    }
    if text.contains("connection refused")
        || text.contains("connection reset")
        || text.contains("unreachable")
        || text.contains("connect timed out")
        || text.contains("econnrefused")
    {
        return NetErrorKind::Tcp;
    }
    NetErrorKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap a message as an `Error::Io`, mirroring how transport errors surface
    /// their cause text without performing any real network I/O.
    fn io_error(msg: &str) -> Error {
        Error::Io(std::io::Error::other(msg.to_owned()))
    }

    #[test]
    fn classifies_dns_failure() {
        assert_eq!(
            classify(&io_error("failed to lookup address information")),
            NetErrorKind::Dns
        );
        assert_eq!(
            classify(&io_error("dns error: no records")),
            NetErrorKind::Dns
        );
    }

    #[test]
    fn classifies_connection_refused_as_tcp() {
        assert_eq!(
            classify(&io_error(
                "tcp connect error: Connection refused (os error 61)"
            )),
            NetErrorKind::Tcp
        );
        assert_eq!(
            classify(&io_error("network is unreachable")),
            NetErrorKind::Tcp
        );
    }

    #[test]
    fn classifies_certificate_failure_as_tls() {
        assert_eq!(
            classify(&io_error("invalid peer certificate: Expired")),
            NetErrorKind::Tls
        );
    }

    #[test]
    fn classifies_unmatched_as_unknown() {
        assert_eq!(
            classify(&io_error("something odd happened")),
            NetErrorKind::Unknown
        );
        // Explicit timeouts are classified regardless of their message.
        assert_eq!(
            classify(&Error::Timeout("waited too long".into())),
            NetErrorKind::Timeout
        );
    }

    #[test]
    fn descriptions_are_non_empty() {
        for kind in [
            NetErrorKind::Dns,
            NetErrorKind::Tcp,
            NetErrorKind::Tls,
            NetErrorKind::Timeout,
            NetErrorKind::Unknown,
        ] {
            assert!(!kind.description().is_empty());
            assert!(!kind.as_str().is_empty());
        }
    }
}
