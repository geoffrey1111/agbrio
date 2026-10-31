//! Native embedded media only, bound to an exact task/session/public entry.
use crate::{host_application::RouterCore, message_media::MediaFile};
use serde_json::Value;
use std::{io::BufReader, path::PathBuf};
pub(crate) fn read(
    core: &RouterCore,
    id: &str,
    turn: Option<&str>,
    item: Option<&str>,
    source: &str,
) -> Result<MediaFile, String> {
    let task = core.store.pi_task(id)?;
    let native = task
        .native_session_id
        .as_deref()
        .ok_or("PI_SESSION_ID_UNCONFIRMED")?;
    if turn != Some(format!("pi-session:{native}").as_str()) {
        return Err("MEDIA_SOURCE_MISMATCH".into());
    }
    let parts = source.split('/').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "pi-media" || item != Some(parts[1]) {
        return Err("PI_MEDIA_REQUIRES_EXACT_NATIVE_IMAGE".into());
    }
    let number = parts[2]
        .split('.')
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n < 16)
        .ok_or("MEDIA_SOURCE_MISMATCH")?;
    let public = core.store.pi_public_item(&task.id, parts[1])?;
    let block = public["content"]
        .as_array()
        .ok_or("PI_HISTORY_INVALID")?
        .iter()
        .filter(|b| b["type"] == "image")
        .nth(number)
        .ok_or("MEDIA_SOURCE_MISMATCH")?;
    let mime = block["mimeType"]
        .as_str()
        .filter(|s| matches!(*s, "image/png" | "image/jpeg"))
        .ok_or("MEDIA_TYPE_UNAVAILABLE")?;
    let extension = if mime == "image/png" { "png" } else { "jpg" };
    if parts[2] != format!("{number}.{extension}") {
        return Err("MEDIA_SOURCE_MISMATCH".into());
    }
    // The first native run may not have flushed its session file yet. An exact
    // consumed request can use the original owner upload, bound by its saved
    // byte digest. This is not a filename/text-match fallback.
    if public["role"] == "user" {
        if let Some(request) = public["clientId"].as_str() {
            if let Ok(input) = core.store.pi_command_input(&task.id, request) {
                let attachments = input["attachments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                let options = crate::watch_chat::ReplyOptions {
                    attachments,
                    ..Default::default()
                };
                let _cache = crate::attachment_cache::guard()?;
                if let Ok(images) = crate::watch_chat::pi_attachment_images(
                    &super::presentation::watch(&task)?,
                    &options,
                ) {
                    if let Some(image) = images.get(number) {
                        use sha2::{Digest, Sha256};
                        let data = image["data"].as_str().ok_or("MEDIA_FILE_UNAVAILABLE")?;
                        if image["mimeType"] == mime
                            && input["images"][number]["encodedSha256"].as_str()
                                == Some(format!("{:x}", Sha256::digest(data.as_bytes())).as_str())
                        {
                            return Ok(MediaFile {
                                filename: format!("Pi-{}-{number}.{extension}", parts[1]),
                                mime: mime.into(),
                                data: data.into(),
                            });
                        }
                    }
                }
            }
        }
    }
    let file = PathBuf::from(
        task.native_session_file
            .as_deref()
            .ok_or("PI_SESSION_FILE_UNCONFIRMED")?,
    );
    let parent = super::permissions::canonical_directory(&super::session::plain(
        file.parent().ok_or("MEDIA_SOURCE_MISMATCH")?,
    ))?;
    let owned = super::permissions::canonical_directory(&super::session::plain(
        &core
            .store
            .pi_storage_root()?
            .join(&task.id)
            .join("sessions"),
    ))?;
    if !parent.starts_with(owned) {
        return Err("MEDIA_SOURCE_MISMATCH".into());
    }
    super::session::verify_session_header(&file, native, std::path::Path::new(&task.directory))?;
    let mut reader =
        BufReader::new(std::fs::File::open(file).map_err(|_| "MEDIA_FILE_UNAVAILABLE")?);
    while let Some(entry) = super::transport::read_record(&mut reader)? {
        if entry["type"] != "message" || entry["id"] != parts[1] {
            continue;
        }
        let image = entry["message"]["content"]
            .as_array()
            .ok_or("MEDIA_SOURCE_MISMATCH")?
            .iter()
            .filter(|v| v["type"] == "image")
            .nth(number)
            .ok_or("MEDIA_SOURCE_MISMATCH")?;
        if image["mimeType"] != mime {
            return Err("MEDIA_SOURCE_MISMATCH".into());
        }
        let data = image["data"].as_str().ok_or("MEDIA_FILE_UNAVAILABLE")?;
        use base64ct::Encoding;
        let bytes = base64ct::Base64::decode_vec(data).map_err(|_| "MEDIA_FILE_UNAVAILABLE")?;
        if bytes.len() > 10 * 1024 * 1024
            || !matches!(
                (mime, image::guess_format(&bytes)),
                ("image/png", Ok(image::ImageFormat::Png))
                    | ("image/jpeg", Ok(image::ImageFormat::Jpeg))
            )
        {
            return Err("MEDIA_TYPE_UNAVAILABLE".into());
        }
        return Ok(MediaFile {
            filename: format!("Pi-{}-{number}.{extension}", parts[1]),
            mime: mime.into(),
            data: data.into(),
        });
    }
    Err("MEDIA_SOURCE_EXPIRED".into())
}
