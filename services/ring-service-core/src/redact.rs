use std::{borrow::Cow, fmt::Display};

pub const API_KEY_PARAMETER: &str = "api-key=";
const SECRET_PARAMETERS: [&str; 4] = [API_KEY_PARAMETER, "token=", "key=", "password="];

/// Upstream errors quote request URLs, a log never carries their API key.
pub fn redacted(error: &impl Display) -> String {
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
    use crate::error::ServiceError;

    #[test]
    fn redacted_masks_url_credentials_and_query_secrets_without_mangling_pubkey() {
        let db = sea_orm::DbErr::Custom(
            "connect postgres://admin:p@ssword123@db.internal:5432/ring?token=tok_secret&key=k_secret&pubkey=KeepMe"
                .to_owned(),
        );
        let logged = redacted(&ServiceError::Store(db));
        assert!(!logged.contains("admin"), "{logged}");
        assert!(!logged.contains("p@ssword123"), "{logged}");
        assert!(!logged.contains("tok_secret"), "{logged}");
        assert!(!logged.contains("k_secret"), "{logged}");
        assert!(
            logged.contains("postgres://redacted@db.internal:5432/ring"),
            "{logged}"
        );
        assert!(logged.contains("token=redacted"), "{logged}");
        assert!(logged.contains("key=redacted"), "{logged}");
        assert!(logged.contains("pubkey=KeepMe"), "{logged}");

        let dsn =
            sea_orm::DbErr::Custom("host=db user=app password=dbsecret dbname=ring".to_owned());
        let logged_dsn = redacted(&ServiceError::Store(dsn));
        assert!(!logged_dsn.contains("dbsecret"), "{logged_dsn}");
        assert!(logged_dsn.contains("password=redacted"), "{logged_dsn}");

        let punctuated = sea_orm::DbErr::Custom(
            "host=db password=my!secret@123$:*^&token='p@ss!word' dbname=ring".to_owned(),
        );
        let logged_punctuated = redacted(&ServiceError::Store(punctuated));
        assert!(
            !logged_punctuated.contains("my!secret@123"),
            "{logged_punctuated}"
        );
        assert!(
            !logged_punctuated.contains("p@ss!word"),
            "{logged_punctuated}"
        );
        assert!(
            logged_punctuated.contains("password=redacted&token='redacted'"),
            "{logged_punctuated}"
        );
    }
}
