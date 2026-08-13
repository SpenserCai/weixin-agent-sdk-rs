//! CDN download and decryption.

use crate::cdn::aes_ecb;
use crate::cdn::cdn_upload::build_cdn_download_url;
use crate::error::{Error, Result};
use crate::types::CdnMedia;
use crate::util::net_error;

/// Resolve the download URL for a CDN media reference.
/// Prefers `full_url`, falls back to constructing from `encrypt_query_param`.
pub fn resolve_cdn_download_url(cdn_base_url: &str, media: &CdnMedia) -> Option<String> {
    if let Some(full) = &media.full_url {
        let trimmed = full.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    media
        .encrypt_query_param
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|p| build_cdn_download_url(cdn_base_url, p))
}

/// Download raw bytes from a URL.
///
/// Transport failures are logged with a redacted URL and a failure category; the
/// raw error text is never logged, since it can carry the full CDN URL including
/// its query string (standards §1.3).
async fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let res = reqwest::get(url).await.map_err(|e| {
        log_download_failure(url, "request", &Error::Http(e));
        Error::CdnUpload("CDN download transport failure".into())
    })?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(Error::CdnUpload(format!("CDN download {status}: {body}")));
    }
    match res.bytes().await {
        Ok(bytes) => Ok(bytes.to_vec()),
        Err(e) => {
            log_download_failure(url, "body", &Error::Http(e));
            Err(Error::CdnUpload(
                "CDN download body transfer failure".into(),
            ))
        }
    }
}

fn log_download_failure(url: &str, stage: &str, err: &Error) {
    let kind = net_error::classify(err);
    tracing::error!(
        stage,
        url = crate::util::redact::redact_url(url),
        kind = kind.as_str(),
        description = kind.description(),
        "CDN download transport failure"
    );
}

/// Download and AES-128-ECB decrypt a CDN media file.
///
/// `aes_key_base64` is the `CdnMedia.aes_key` field (see [`aes_ecb::parse_aes_key`] for formats).
pub async fn download_and_decrypt(
    cdn_base_url: &str,
    media: &CdnMedia,
    aes_key_base64: &str,
) -> Result<Vec<u8>> {
    let key = aes_ecb::parse_aes_key(aes_key_base64)?;
    let url = resolve_cdn_download_url(cdn_base_url, media)
        .ok_or_else(|| Error::CdnUpload("no download URL available".into()))?;
    tracing::debug!(url = %crate::util::redact::redact_url(&url), "CDN download+decrypt");
    let encrypted = fetch_bytes(&url).await?;
    aes_ecb::decrypt(&encrypted, &key)
}

/// Download plain (unencrypted) bytes from the CDN.
pub async fn download_plain(cdn_base_url: &str, media: &CdnMedia) -> Result<Vec<u8>> {
    let url = resolve_cdn_download_url(cdn_base_url, media)
        .ok_or_else(|| Error::CdnUpload("no download URL available".into()))?;
    tracing::debug!(url = %crate::util::redact::redact_url(&url), "CDN download plain");
    fetch_bytes(&url).await
}
