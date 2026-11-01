//! Photos published in posts, like on Pixelfed: their format and size are
//! read from the file, and what the camera wrote in it (the place it was
//! taken, the phone, the date, comments...) is removed before it's saved
//! and sent to other servers. Only the rotation is kept, else phone photos
//! would show sideways.
//!
//! JPEG, PNG, GIF and WebP only: other formats can't be cleaned here.

/// A cleaned photo, ready to be saved.
#[derive(Debug, PartialEq, Eq)]
pub struct Photo {
    pub data: Vec<u8>,
    pub extension: &'static str,
    pub media_type: &'static str,
    /// As shown, after the rotation.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PhotoError {
    /// Not JPEG, PNG, GIF or WebP.
    Unsupported,
    /// Cut or broken.
    Invalid,
}

pub fn clean(data: &[u8]) -> Result<Photo, PhotoError> {
    let (data, extension, media_type, (width, height)) = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let (cleaned, size) = clean_jpeg(data)?;
        (cleaned, "jpg", "image/jpeg", size)
    } else if data.starts_with(PNG_SIGNATURE) {
        let (cleaned, size) = clean_png(data)?;
        (cleaned, "png", "image/png", size)
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        // No camera information in GIFs.
        let size = gif_size(data)?;
        (data.to_vec(), "gif", "image/gif", size)
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        let (cleaned, size) = clean_webp(data)?;
        (cleaned, "webp", "image/webp", size)
    } else {
        return Err(PhotoError::Unsupported);
    };
    if width == 0 || height == 0 {
        return Err(PhotoError::Invalid);
    }
    Ok(Photo { data, extension, media_type, width, height })
}

fn u16_be(data: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32)
}

fn u32_be(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn u16_le(data: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32)
}

fn u24_le(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at + 3)?;
    Some(bytes[0] as u32 | (bytes[1] as u32) << 8 | (bytes[2] as u32) << 16)
}

fn u32_le(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

// --- JPEG ---

/// Segments kept: JFIF (APP0), the colour profile (APP2) and Adobe's colour
/// information (APP14). The other APPn (EXIF, XMP, IPTC, makers' notes) and
/// comments are removed.
fn keep_jpeg_segment(marker: u8) -> bool {
    !matches!(marker, 0xE1 | 0xE3..=0xED | 0xEF | 0xFE)
}

/// Frame headers, where the size is: SOF0 to SOF15, except DHT, JPG and DAC.
fn is_frame_header(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC)
}

fn clean_jpeg(data: &[u8]) -> Result<(Vec<u8>, (u32, u32)), PhotoError> {
    let mut cleaned = Vec::with_capacity(data.len());
    cleaned.extend_from_slice(&data[0..2]);
    let mut at = 2;
    let mut size = None;
    let mut orientation = 1;
    loop {
        // Fill bytes before a marker are allowed.
        while data.get(at) == Some(&0xFF) && data.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if data.get(at) != Some(&0xFF) {
            return Err(PhotoError::Invalid);
        }
        let marker = *data.get(at + 1).ok_or(PhotoError::Invalid)?;
        // Markers without content.
        if matches!(marker, 0x01 | 0xD0..=0xD7) {
            cleaned.extend_from_slice(&data[at..at + 2]);
            at += 2;
            continue;
        }
        if marker == 0xD9 {
            cleaned.extend_from_slice(&data[at..at + 2]);
            break;
        }
        let length = u16_be(data, at + 2).ok_or(PhotoError::Invalid)? as usize;
        if length < 2 {
            return Err(PhotoError::Invalid);
        }
        let end = at + 2 + length;
        let segment = data.get(at..end).ok_or(PhotoError::Invalid)?;
        let content = &segment[4..];
        if is_frame_header(marker) {
            size = Some((u16_be(content, 3).ok_or(PhotoError::Invalid)?, u16_be(content, 1).ok_or(PhotoError::Invalid)?));
        }
        if marker == 0xE1 && content.starts_with(b"Exif\0\0") {
            orientation = exif_orientation(&content[6..]).unwrap_or(1);
            // In place of the EXIF: only the rotation.
            if orientation != 1 {
                cleaned.extend_from_slice(&orientation_segment(orientation));
            }
        } else if keep_jpeg_segment(marker) {
            cleaned.extend_from_slice(segment);
        }
        at = end;
        // The picture itself, up to the end: copied as it is.
        if marker == 0xDA {
            cleaned.extend_from_slice(&data[at..]);
            break;
        }
    }
    let (width, height) = size.ok_or(PhotoError::Invalid)?;
    // 5 to 8 turn the photo a quarter.
    Ok((cleaned, if (5..=8).contains(&orientation) { (height, width) } else { (width, height) }))
}

/// The Orientation tag (0x0112) of the first IFD of an EXIF block.
fn exif_orientation(tiff: &[u8]) -> Option<u16> {
    let big_endian = match tiff.get(0..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let read16 = |at: usize| if big_endian { u16_be(tiff, at) } else { u16_le(tiff, at) };
    let read32 = |at: usize| if big_endian { u32_be(tiff, at) } else { u32_le(tiff, at) };
    let ifd = read32(4)? as usize;
    let count = read16(ifd)? as usize;
    for index in 0..count.min(512) {
        let entry = ifd + 2 + index * 12;
        if read16(entry)? == 0x0112 {
            let value = read16(entry + 8)? as u16;
            return (1..=8).contains(&value).then_some(value);
        }
    }
    None
}

/// An APP1 segment with an EXIF block holding only the orientation.
fn orientation_segment(orientation: u16) -> Vec<u8> {
    let mut segment = vec![0xFF, 0xE1, 0, 34];
    segment.extend_from_slice(b"Exif\0\0MM\0\x2A\0\0\0\x08");
    // One entry: Orientation, SHORT, 1 value.
    segment.extend_from_slice(&[0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1]);
    segment.extend_from_slice(&orientation.to_be_bytes());
    // Padding of the value, then no next IFD.
    segment.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    segment
}

// --- PNG ---

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// EXIF, texts (where programs write the camera, the author...) and the date.
fn drop_png_chunk(kind: &[u8]) -> bool {
    matches!(kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME")
}

fn clean_png(data: &[u8]) -> Result<(Vec<u8>, (u32, u32)), PhotoError> {
    let mut cleaned = Vec::with_capacity(data.len());
    cleaned.extend_from_slice(PNG_SIGNATURE);
    let mut at = PNG_SIGNATURE.len();
    let mut size = None;
    while at < data.len() {
        let length = u32_be(data, at).ok_or(PhotoError::Invalid)? as usize;
        let end = at.checked_add(12 + length).ok_or(PhotoError::Invalid)?;
        let chunk = data.get(at..end).ok_or(PhotoError::Invalid)?;
        let kind = &chunk[4..8];
        if kind == b"IHDR" {
            size = Some((u32_be(chunk, 8).ok_or(PhotoError::Invalid)?, u32_be(chunk, 12).ok_or(PhotoError::Invalid)?));
        }
        if !drop_png_chunk(kind) {
            cleaned.extend_from_slice(chunk);
        }
        at = end;
        if kind == b"IEND" {
            break;
        }
    }
    Ok((cleaned, size.ok_or(PhotoError::Invalid)?))
}

// --- GIF ---

fn gif_size(data: &[u8]) -> Result<(u32, u32), PhotoError> {
    Ok((u16_le(data, 6).ok_or(PhotoError::Invalid)?, u16_le(data, 8).ok_or(PhotoError::Invalid)?))
}

// --- WebP ---

fn clean_webp(data: &[u8]) -> Result<(Vec<u8>, (u32, u32)), PhotoError> {
    let mut chunks = Vec::with_capacity(data.len());
    let mut at = 12;
    let mut size = None;
    let end_of_file = (u32_le(data, 4).ok_or(PhotoError::Invalid)? as usize + 8).min(data.len());
    while at + 8 <= end_of_file {
        let kind = &data[at..at + 4];
        let length = u32_le(data, at + 4).ok_or(PhotoError::Invalid)? as usize;
        // Chunks are padded to an even length.
        let end = at.checked_add(8 + length + length % 2).ok_or(PhotoError::Invalid)?.min(end_of_file);
        let chunk = data.get(at..end).ok_or(PhotoError::Invalid)?;
        let content = data.get(at + 8..at + 8 + length).ok_or(PhotoError::Invalid)?;
        match kind {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                size = Some((
                    u24_le(content, 4).ok_or(PhotoError::Invalid)? + 1,
                    u24_le(content, 7).ok_or(PhotoError::Invalid)? + 1,
                ));
                let mut chunk = chunk.to_vec();
                // The flags saying there is EXIF (0x08) or XMP (0x04).
                chunk[8] &= !0x0C;
                chunks.extend_from_slice(&chunk);
            }
            b"VP8 " => {
                if size.is_none() {
                    size = Some((
                        u16_le(content, 6).ok_or(PhotoError::Invalid)? & 0x3FFF,
                        u16_le(content, 8).ok_or(PhotoError::Invalid)? & 0x3FFF,
                    ));
                }
                chunks.extend_from_slice(chunk);
            }
            b"VP8L" => {
                if size.is_none() {
                    let bits = u32_le(content, 1).ok_or(PhotoError::Invalid)?;
                    size = Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1));
                }
                chunks.extend_from_slice(chunk);
            }
            _ => chunks.extend_from_slice(chunk),
        }
        at = end;
    }
    let mut cleaned = Vec::with_capacity(chunks.len() + 12);
    cleaned.extend_from_slice(b"RIFF");
    cleaned.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
    cleaned.extend_from_slice(b"WEBP");
    cleaned.extend_from_slice(&chunks);
    Ok((cleaned, size.ok_or(PhotoError::Invalid)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(marker: u8, content: &[u8]) -> Vec<u8> {
        let mut segment = vec![0xFF, marker];
        segment.extend_from_slice(&((content.len() + 2) as u16).to_be_bytes());
        segment.extend_from_slice(content);
        segment
    }

    /// An EXIF block, little endian like most phones, with the orientation
    /// and a GPS pointer.
    fn exif(orientation: u16) -> Vec<u8> {
        let mut block = b"Exif\0\0II\x2A\0\x08\0\0\0".to_vec();
        block.extend_from_slice(&[2, 0]);
        block.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
        block.extend_from_slice(&orientation.to_le_bytes());
        block.extend_from_slice(&[0, 0]);
        block.extend_from_slice(&[0x25, 0x88, 4, 0, 1, 0, 0, 0, 0x26, 0, 0, 0]);
        block.extend_from_slice(&[0, 0, 0, 0]);
        block.extend_from_slice(b"GPS 48.85N 2.35E");
        block
    }

    fn jpeg(orientation: u16) -> Vec<u8> {
        let mut data = vec![0xFF, 0xD8];
        data.extend(segment(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0"));
        data.extend(segment(0xE1, &exif(orientation)));
        data.extend(segment(0xFE, b"Taken by Alice"));
        // 8 bits, 300 high, 400 wide, 1 component.
        data.extend(segment(0xC0, &[8, 0x01, 0x2C, 0x01, 0x90, 1, 1, 0x11, 0]));
        data.extend(segment(0xDA, &[1, 1, 0, 0, 0x3F, 0]));
        data.extend_from_slice(&[0x12, 0x34, 0xFF, 0x00, 0x56, 0xFF, 0xD9]);
        data
    }

    fn contains(data: &[u8], needle: &[u8]) -> bool {
        data.windows(needle.len()).any(|window| window == needle)
    }

    #[test]
    fn jpeg_loses_its_metadata_but_not_its_rotation() {
        let photo = clean(&jpeg(1)).unwrap();
        assert_eq!((photo.width, photo.height, photo.extension), (400, 300, "jpg"));
        assert!(!contains(&photo.data, b"GPS") && !contains(&photo.data, b"Alice") && !contains(&photo.data, b"Exif"));
        assert!(contains(&photo.data, b"JFIF") && photo.data.ends_with(&[0x12, 0x34, 0xFF, 0x00, 0x56, 0xFF, 0xD9]));

        // Turned a quarter: the size is swapped and the rotation stays.
        let photo = clean(&jpeg(6)).unwrap();
        assert_eq!((photo.width, photo.height), (300, 400));
        assert!(!contains(&photo.data, b"GPS"));
        let at = photo.data.windows(6).position(|window| window == b"Exif\0\0").unwrap();
        assert_eq!(exif_orientation(&photo.data[at + 6..]), Some(6));
    }

    #[test]
    fn png_loses_its_texts() {
        let chunk = |kind: &[u8], content: &[u8]| {
            let mut chunk = (content.len() as u32).to_be_bytes().to_vec();
            chunk.extend_from_slice(kind);
            chunk.extend_from_slice(content);
            chunk.extend_from_slice(&[0, 0, 0, 0]);
            chunk
        };
        let mut data = PNG_SIGNATURE.to_vec();
        data.extend(chunk(b"IHDR", &[0, 0, 2, 0, 0, 0, 1, 0, 8, 6, 0, 0, 0]));
        data.extend(chunk(b"tEXt", b"Author\0Alice"));
        data.extend(chunk(b"eXIf", b"MM\0\x2AGPS"));
        data.extend(chunk(b"IDAT", b"pixels"));
        data.extend(chunk(b"IEND", b""));
        let photo = clean(&data).unwrap();
        assert_eq!((photo.width, photo.height), (512, 256));
        assert!(!contains(&photo.data, b"Alice") && !contains(&photo.data, b"GPS"));
        assert!(contains(&photo.data, b"pixels") && contains(&photo.data, b"IEND"));
    }

    #[test]
    fn webp_loses_its_exif() {
        let chunk = |kind: &[u8], content: &[u8]| {
            let mut chunk = kind.to_vec();
            chunk.extend_from_slice(&(content.len() as u32).to_le_bytes());
            chunk.extend_from_slice(content);
            if content.len() % 2 == 1 {
                chunk.push(0);
            }
            chunk
        };
        // Flags with EXIF, 640 × 480.
        let mut chunks = chunk(b"VP8X", &[0x08, 0, 0, 0, 0x7F, 0x02, 0, 0xDF, 0x01, 0]);
        chunks.extend(chunk(b"VP8L", &[0x2F, 0, 0, 0, 0, 1]));
        chunks.extend(chunk(b"EXIF", b"GPS 48N"));
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
        data.extend_from_slice(b"WEBP");
        data.extend(chunks);
        let photo = clean(&data).unwrap();
        assert_eq!((photo.width, photo.height, photo.media_type), (640, 480, "image/webp"));
        assert!(!contains(&photo.data, b"GPS"));
        assert_eq!(photo.data[20] & 0x08, 0);
        assert_eq!(u32_le(&photo.data, 4).unwrap() as usize, photo.data.len() - 8);
    }

    #[test]
    fn other_formats_are_refused() {
        assert_eq!(clean(b"<svg onload=alert(1)>"), Err(PhotoError::Unsupported));
        assert_eq!(clean(&[0xFF, 0xD8, 0xFF, 0xE0, 0]), Err(PhotoError::Invalid));
        assert_eq!(clean(b"GIF89a\x10\0\x20\0").map(|photo| (photo.width, photo.height)), Ok((16, 32)));
    }
}
