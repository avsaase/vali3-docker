use std::io::Read;

use bytes::Bytes;
use zip::ZipArchive;

use crate::error::ValiError;

pub fn is_igc(data: &Bytes) -> bool {
    data.len() >= 4
        && data[0] == b'A'
        && data[1].is_ascii_uppercase()
        && data[2..4]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b',')
}

pub fn unpack(data: Bytes, max_size: usize) -> Result<Bytes, ValiError> {
    if is_igc(&data) {
        return Ok(data);
    }

    let inner = if data.starts_with(b"PK\x03\x04") {
        read_zip(&data, max_size)?
    } else if data.starts_with(b"\x1f\x8b\x08") {
        read_gzip(&data, max_size)?
    } else {
        tracing::warn!("uploaded file is not an IGC, zip, or gzip file");
        return Err(ValiError::Format);
    };

    if is_igc(&inner) {
        Ok(inner)
    } else {
        tracing::warn!("decompressed file is not a valid IGC file");
        Err(ValiError::Format)
    }
}

fn read_zip(data: &Bytes, max_size: usize) -> Result<Bytes, ValiError> {
    let mut zip = ZipArchive::new(std::io::Cursor::new(data)).map_err(|error| {
        tracing::warn!(%error, "failed to parse zip file");
        ValiError::FormatZip
    })?;
    let mut entry = zip.by_index(0).map_err(|error| {
        tracing::warn!(%error, "failed to read first zip entry");
        ValiError::FormatZip
    })?;

    // The entry's declared size is cheap to check and catches most zip bombs before we start
    // decompressing anything.
    if entry.size() > max_size as u64 {
        tracing::warn!(
            size = entry.size(),
            max_size,
            "uncompressed zip entry exceeds maximum size"
        );
        return Err(ValiError::Size { max_size });
    }

    match read_limited(&mut entry, max_size) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => {
            tracing::warn!(max_size, "uncompressed zip entry exceeds maximum size");
            Err(ValiError::Size { max_size })
        }
        Err(error) => {
            tracing::warn!(%error, "failed to decompress zip entry");
            Err(ValiError::FormatZip)
        }
    }
}

fn read_gzip(data: &Bytes, max_size: usize) -> Result<Bytes, ValiError> {
    let mut decoder = flate2::read::GzDecoder::new(&data[..]);

    match read_limited(&mut decoder, max_size) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => {
            tracing::warn!(max_size, "uncompressed gzip data exceeds maximum size");
            Err(ValiError::Size { max_size })
        }
        Err(error) => {
            tracing::warn!(%error, "failed to decompress gzip data");
            Err(ValiError::FormatGzip)
        }
    }
}

/// Read at most `max_size` bytes. Returns `None` when the reader produces more data than that,
/// which prevents a small compressed upload from expanding into an arbitrarily large allocation.
fn read_limited(reader: &mut impl Read, max_size: usize) -> anyhow::Result<Option<Bytes>> {
    let limit = (max_size as u64).saturating_add(1);
    let mut out = Vec::new();
    reader.take(limit).read_to_end(&mut out)?;

    if out.len() > max_size {
        Ok(None)
    } else {
        Ok(Some(out.into()))
    }
}
