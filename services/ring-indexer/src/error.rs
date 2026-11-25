use std::{borrow::Cow, fmt::Display};

use jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned};
use thiserror::Error;
use zolana_ring_indexer::rpc::RingRpcError;

const SECRET_PARAMETERS: [&str; 4] = ["api-key=", "token=", "key=", "password="];

#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum RingApiError {
    #[error("member registry is out of sync ({0})")]
    OutOfSync(String),
    #[error("member registry root changed")]
    RootChanged,
    #[error("member registry page has an index gap or inconsistent leaf")]
    PageGap,
    #[error("invalid request ({0})")]
    InvalidRequest(String),
    #[error("internal error ({0})")]
    Internal(String),
}

impl From<sea_orm::DbErr> for RingApiError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<RingApiError> for ErrorObjectOwned {
    fn from(error: RingApiError) -> Self {
        let refusal = match &error {
            RingApiError::OutOfSync(_) => RingRpcError::RegistryOutOfSync,
            RingApiError::RootChanged => RingRpcError::RegistryRootChanged,
            RingApiError::PageGap => RingRpcError::RegistryPageGap,
            RingApiError::InvalidRequest(_) => {
                return Self::owned(
                    ErrorCode::InvalidRequest.code(),
                    error.to_string(),
                    None::<()>,
                )
            }
            RingApiError::Internal(reason) => {
                log::error!("ring lookup failed ({})", redacted(reason));
                return Self::owned(
                    ErrorCode::InternalError.code(),
                    "internal server error",
                    None::<()>,
                );
            }
        };
        Self::owned(refusal.code(), error.to_string(), None::<()>)
    }
}

pub(crate) fn redacted(error: &impl Display) -> String {
    redact_secrets(&error.to_string()).into_owned()
}

fn redact_secrets(text: &str) -> Cow<'_, str> {
    match redact_url_credentials(text) {
        Cow::Borrowed(text) => redact_parameters(text),
        Cow::Owned(masked) => Cow::Owned(redact_parameters(&masked).into_owned()),
    }
}

fn redact_url_credentials(text: &str) -> Cow<'_, str> {
    if !text.contains("://") || !text.contains('@') {
        return Cow::Borrowed(text);
    }
    let mut redacted = String::with_capacity(text.len());
    let mut start = 0;
    let mut changed = false;
    while let Some(found) = text.get(start..).and_then(|rest| rest.find("://")) {
        let authority_start = start + found + 3;
        let rest = text.get(authority_start..).unwrap_or_default();
        let authority_len = rest
            .find(|c: char| {
                matches!(c, '/' | '?' | '#' | '"' | '\'' | ')' | ']' | '>') || c.is_whitespace()
            })
            .unwrap_or(rest.len());
        let authority = rest.get(..authority_len).unwrap_or_default();
        if let Some(at) = authority.rfind('@') {
            redacted.push_str(text.get(start..authority_start).unwrap_or_default());
            redacted.push_str("redacted");
            start = authority_start + at;
            changed = true;
        } else {
            let next = authority_start + authority_len;
            redacted.push_str(text.get(start..next).unwrap_or_default());
            start = next;
        }
    }
    if !changed {
        return Cow::Borrowed(text);
    }
    redacted.push_str(text.get(start..).unwrap_or_default());
    Cow::Owned(redacted)
}

#[derive(Clone, Copy)]
struct SecretParameter {
    offset: usize,
    len: usize,
}

fn redact_parameters(text: &str) -> Cow<'_, str> {
    let lowercase = text.to_ascii_lowercase();
    if !SECRET_PARAMETERS
        .iter()
        .any(|parameter| lowercase.contains(parameter))
    {
        return Cow::Borrowed(text);
    }
    let mut redacted = String::with_capacity(text.len());
    let mut start = 0;
    let mut changed = false;
    while let Some(found) = next_secret_parameter(&lowercase, start) {
        let mut value_start = found.offset + found.len;
        let mut rest = text.get(value_start..).unwrap_or_default();
        let value_len = match rest.chars().next().filter(|c| matches!(c, '"' | '\'')) {
            Some(quote) => {
                value_start += 1;
                rest = text.get(value_start..).unwrap_or_default();
                rest.find(quote).unwrap_or(rest.len())
            }
            None => rest
                .find(|c: char| {
                    matches!(c, '&' | '"' | '\'' | '#' | ')' | ']' | '>') || c.is_whitespace()
                })
                .unwrap_or(rest.len()),
        };
        redacted.push_str(text.get(start..value_start).unwrap_or_default());
        redacted.push_str("redacted");
        start = value_start + value_len;
        changed = true;
    }
    if !changed {
        return Cow::Borrowed(text);
    }
    redacted.push_str(text.get(start..).unwrap_or_default());
    Cow::Owned(redacted)
}

fn next_secret_parameter(lowercase: &str, start: usize) -> Option<SecretParameter> {
    let bytes = lowercase.as_bytes();
    let mut best: Option<SecretParameter> = None;
    for parameter in SECRET_PARAMETERS {
        let mut cursor = start;
        while let Some(found) = lowercase
            .get(cursor..)
            .and_then(|rest| rest.find(parameter))
        {
            let offset = cursor + found;
            let boundary = offset == 0
                || bytes.get(offset - 1).is_some_and(|byte| {
                    !(byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
                });
            if boundary {
                if best.is_none_or(|current| {
                    offset < current.offset
                        || (offset == current.offset && parameter.len() > current.len)
                }) {
                    best = Some(SecretParameter {
                        offset,
                        len: parameter.len(),
                    });
                }
                break;
            }
            cursor = offset + 1;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_masks_urls_and_secret_parameters() {
        let text = "connect postgres://admin:p@ss!123@db.internal:5432/ring?api-key=f00d&password=my!secret@123&pubkey=KeepMe";
        let masked = redacted(&text);
        assert!(!masked.contains("admin"), "{masked}");
        assert!(!masked.contains("p@ss!123"), "{masked}");
        assert!(!masked.contains("f00d"), "{masked}");
        assert!(!masked.contains("my!secret@123"), "{masked}");
        assert!(
            masked.contains("postgres://redacted@db.internal:5432/ring"),
            "{masked}"
        );
        assert!(masked.contains("api-key=redacted"), "{masked}");
        assert!(masked.contains("password=redacted"), "{masked}");
        assert!(masked.contains("pubkey=KeepMe"), "{masked}");

        let internal = ErrorObjectOwned::from(RingApiError::Internal(text.to_owned()));
        assert_eq!(internal.code(), ErrorCode::InternalError.code());
        assert_eq!(internal.message(), "internal server error");
    }
}
