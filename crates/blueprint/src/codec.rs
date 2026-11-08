use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

use crate::error::BlueprintError;
use crate::types::BlueprintData;

/// Upper bound on decompressed blueprint JSON. Zlib compresses ~1000:1, so a
/// tiny pasted string can expand to gigabytes (a "zip bomb"). The decoder is
/// capped at this many bytes so a malicious or corrupt blueprint can't OOM or
/// freeze the app — decode fails fast with `DecompressedTooLarge` instead. Real
/// blueprints decompress to a few MB at most; 64 MiB is comfortably generous.
pub const DECOMPRESSED_LIMIT: usize = 64 * 1024 * 1024;

/// Upper bound on the compressed payload (base64-decoded, pre-inflate).
/// `DECOMPRESSED_LIMIT` bounds how far a paste can *expand*, but base64 decoding
/// runs first and allocates ~3/4 of the pasted string up front — so without this
/// cap a multi-hundred-MB paste could exhaust memory before the zlib guard ever
/// ran, defeating the same threat model. Even a large blueprint book compresses
/// to well under a megabyte, so 16 MiB leaves orders of magnitude of headroom.
pub const COMPRESSED_LIMIT: usize = 16 * 1024 * 1024;

/// Decode a Factorio blueprint string into a `BlueprintData` struct.
///
/// Pipeline: strip version byte → base64 decode → zlib decompress → JSON parse.
pub fn decode(blueprint_string: &str) -> Result<BlueprintData, BlueprintError> {
    let json = decode_to_json(blueprint_string)?;
    let data: BlueprintData = serde_json::from_str(&json)?;
    Ok(data)
}

/// Decode a Factorio blueprint string into raw JSON (for debugging/pretty-printing).
///
/// Pipeline: strip version byte → base64 decode → zlib decompress → return JSON string.
pub fn decode_to_json(blueprint_string: &str) -> Result<String, BlueprintError> {
    if blueprint_string.is_empty() {
        return Err(BlueprintError::MissingVersionByte);
    }

    let version_byte = blueprint_string.chars().next().unwrap();
    if version_byte != '0' {
        return Err(BlueprintError::UnsupportedVersion(version_byte));
    }

    let encoded = &blueprint_string[1..];

    // Reject an oversized paste before decoding it: base64 packs 3 bytes into 4
    // chars, so `len / 4 * 3` is an exact upper bound on what `decode` would
    // allocate. Checking here means a gigabyte-scale paste costs one integer
    // comparison instead of a gigabyte-scale buffer (plus everything downstream).
    if encoded.len() / 4 * 3 > COMPRESSED_LIMIT {
        return Err(BlueprintError::CompressedTooLarge {
            limit: COMPRESSED_LIMIT,
        });
    }

    let compressed = STANDARD.decode(encoded)?;

    // Cap decompression at DECOMPRESSED_LIMIT: read one byte past the limit and,
    // if the reader still had data, reject as a zip bomb. `take` bounds the work
    // to LIMIT+1 bytes regardless of how large the stream would expand to, so
    // memory stays bounded even for adversarial input. Read into a byte buffer
    // (not read_to_string) so the size check runs before UTF-8 validation and an
    // oversized paste reports DecompressedTooLarge rather than a spurious
    // invalid-UTF-8 error from a mid-character truncation.
    let mut decoder = ZlibDecoder::new(&compressed[..]).take(DECOMPRESSED_LIMIT as u64 + 1);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    if buf.len() > DECOMPRESSED_LIMIT {
        return Err(BlueprintError::DecompressedTooLarge {
            limit: DECOMPRESSED_LIMIT,
        });
    }

    let json = String::from_utf8(buf).map_err(|e| {
        BlueprintError::Zlib(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;

    Ok(json)
}

/// Encode a `BlueprintData` struct into a Factorio blueprint string.
///
/// Pipeline: JSON serialize → zlib compress (level 9) → base64 encode → prepend '0'.
pub fn encode(data: &BlueprintData) -> Result<String, BlueprintError> {
    let json = serde_json::to_string(data)?;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json.as_bytes())?;
    let compressed = encoder.finish()?;

    let encoded = STANDARD.encode(&compressed);

    Ok(format!("0{encoded}"))
}

#[cfg(test)]
mod tests;
