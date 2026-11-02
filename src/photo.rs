//! Photos published in posts, like on Pixelfed: their format and size are
//! read from the file, and what the camera wrote in it (the place it was
//! taken, the phone, the date, comments...) is removed before it's saved
//! and sent to other servers. Only the rotation is kept, else phone photos
//! would show sideways.
//!
//! JPEG, PNG, GIF and WebP only: other formats can't be cleaned here.
//!
//! Then `prepare` makes them smaller with ffmpeg (see `compress`), in the
//! same format, shown by every browser since 2015: JPEG re-encoded at a
//! quality where nothing changes to the eye, PNG compressed again without
//! losing anything. The original is kept when that isn't smaller.

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

/// A photo cleaned (see `clean`), then made smaller (see `compress`).
pub async fn prepare(data: &[u8]) -> Result<Photo, PhotoError> {
    Ok(compress(clean(data)?).await)
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

// --- Compression ---

/// The qualities of ffmpeg a JPEG is re-encoded at, the best first (1 is
/// best, 31 worst), until one is small enough (`KEPT_UNDER_PERCENT`). Both
/// keep a SSIM of 0.99 or more on photos of phones and cameras: no visible
/// change. Lower ones (3: 0.975) can be seen.
const JPEG_QUALITIES: [&str; 2] = ["1", "2"];

/// Kept only when at least 10 % smaller (percent of the original): below, a
/// JPEG would lose quality for little.
const KEPT_UNDER_PERCENT: usize = 90;

/// Compressions running at once: they use a core each.
static COMPRESSIONS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

/// Longer, the photo stays as it is.
const COMPRESSION_SECONDS: u64 = 30;

/// Color information of a PNG, copied as it was: the pixels mean the same.
fn is_png_color_chunk(kind: &[u8]) -> bool {
    matches!(kind, b"iCCP" | b"sRGB" | b"gAMA" | b"cHRM" | b"cICP")
}

/// The photo made smaller by ffmpeg, same format and size, or as it was
/// (GIF and WebP, an animated PNG, a CMYK JPEG, no ffmpeg, or not 10 %
/// smaller).
pub async fn compress(photo: Photo) -> Photo {
    let Some(tools) = crate::av::tools() else {
        return photo;
    };
    let small_enough = |data: &Vec<u8>| data.len() * 100 <= photo.data.len() * KEPT_UNDER_PERCENT;
    let mut compressed = None;
    match photo.extension {
        "jpg" => {
            for quality in JPEG_QUALITIES {
                if let Some(data) = compress_jpeg(&tools.ffmpeg, &photo.data, quality).await.filter(small_enough) {
                    compressed = Some(data);
                    break;
                }
            }
        }
        "png" => compressed = compress_png(&tools.ffmpeg, &photo.data).await.filter(small_enough),
        _ => {}
    }
    match compressed {
        Some(data) => {
            tracing::debug!("photo compressed from {} to {} bytes", photo.data.len(), data.len());
            Photo { data, ..photo }
        }
        None => photo,
    }
}

/// Runs ffmpeg on `input` from its standard input, what it writes back.
async fn ffmpeg_pipe(ffmpeg: &str, args: &[&str], input: &[u8]) -> Option<Vec<u8>> {
    use tokio::io::AsyncWriteExt;
    let _turn = COMPRESSIONS.acquire().await.ok()?;
    let mut child = tokio::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error"])
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let input = input.to_vec();
    // Written while the output is read: neither waits for the other.
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
    });
    let output = tokio::time::timeout(std::time::Duration::from_secs(COMPRESSION_SECONDS), child.wait_with_output()).await.ok()?.ok()?;
    let _ = writer.await;
    (output.status.success() && !output.stdout.is_empty()).then_some(output.stdout)
}

/// The segments before the picture of a cleaned JPEG: (marker, whole segment).
fn jpeg_segments(data: &[u8]) -> Vec<(u8, &[u8])> {
    let mut segments = Vec::new();
    let mut at = 2;
    while let (Some(&0xFF), Some(&marker)) = (data.get(at), data.get(at + 1)) {
        if marker == 0xDA {
            break;
        }
        let Some(length) = u16_be(data, at + 2).map(|length| length as usize).filter(|length| *length >= 2) else {
            break;
        };
        let Some(segment) = data.get(at..at + 2 + length) else {
            break;
        };
        segments.push((marker, segment));
        at += 2 + length;
    }
    segments
}

/// JPEG: re-encoded by ffmpeg at `quality` (see `JPEG_QUALITIES`). The
/// rotation and the color profile are put back as they were, untouched.
async fn compress_jpeg(ffmpeg: &str, data: &[u8], quality: &str) -> Option<Vec<u8>> {
    let segments = jpeg_segments(data);
    // Adobe's (CMYK, or colors not in YCbCr): ffmpeg would change them.
    if segments.iter().any(|(marker, _)| *marker == 0xEE) {
        return None;
    }
    let is_profile = |segment: &[u8]| segment.get(4..16) == Some(b"ICC_PROFILE\0".as_slice());
    let kept: Vec<u8> = segments
        .iter()
        .filter(|(marker, segment)| *marker == 0xE1 || (*marker == 0xE2 && is_profile(segment)))
        .flat_map(|(_, segment)| segment.iter().copied())
        .collect();
    // Without its rotation, so ffmpeg leaves the pixels as they are.
    let mut input = data.to_vec();
    if let Some((_, rotation)) = segments.iter().find(|(marker, _)| *marker == 0xE1) {
        let start = rotation.as_ptr() as usize - data.as_ptr() as usize;
        input.drain(start..start + rotation.len());
    }
    // Its color sampling kept (ffmpeg would make 4:2:0 of a 4:2:2 photo).
    let sampling = segments
        .iter()
        .find(|(marker, _)| is_frame_header(*marker))
        .filter(|(_, frame)| frame.get(9) == Some(&3))
        .and_then(|(_, frame)| match frame.get(11) {
            Some(0x22) => Some("yuvj420p"),
            Some(0x21) => Some("yuvj422p"),
            Some(0x11) => Some("yuvj444p"),
            _ => None,
        });
    let mut args = vec!["-f", "jpeg_pipe", "-i", "pipe:0", "-map_metadata", "-1", "-c:v", "mjpeg", "-qmin", "1", "-q:v", quality, "-huffman", "optimal"];
    if let Some(sampling) = sampling {
        args.extend(["-pix_fmt", sampling]);
    }
    args.extend(["-f", "image2pipe", "pipe:1"]);
    let encoded = ffmpeg_pipe(ffmpeg, &args, &input).await?;
    if !encoded.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // ffmpeg's segments, without the profile it may copy (the original goes
    // back instead) nor its comment naming itself; the JFIF header first.
    let segments = jpeg_segments(&encoded);
    let header_end = 2 + segments.iter().map(|(_, segment)| segment.len()).sum::<usize>();
    let mut result = Vec::with_capacity(encoded.len() + kept.len());
    result.extend_from_slice(&encoded[..2]);
    for (_, segment) in segments.iter().filter(|(marker, _)| *marker == 0xE0) {
        result.extend_from_slice(segment);
    }
    result.extend_from_slice(&kept);
    for (_, segment) in segments.iter().filter(|(marker, segment)| *marker != 0xE0 && *marker != 0xFE && !(*marker == 0xE2 && is_profile(segment))) {
        result.extend_from_slice(segment);
    }
    result.extend_from_slice(encoded.get(header_end..)?);
    Some(result)
}

/// The chunks of a PNG: (kind, whole chunk), `None` when it's broken.
fn png_chunks(data: &[u8]) -> Option<Vec<(&[u8], &[u8])>> {
    let mut chunks = Vec::new();
    let mut at = PNG_SIGNATURE.len();
    while at < data.len() {
        let length = u32_be(data, at)? as usize;
        let chunk = data.get(at..at.checked_add(12 + length)?)?;
        chunks.push((&chunk[4..8], chunk));
        at += 12 + length;
    }
    Some(chunks)
}

/// PNG: compressed again by ffmpeg, each line with the best filter, without
/// losing a pixel. The color information is put back as it was.
async fn compress_png(ffmpeg: &str, data: &[u8]) -> Option<Vec<u8>> {
    let chunks = png_chunks(data)?;
    // An animated PNG: ffmpeg would keep one picture.
    if chunks.iter().any(|(kind, _)| *kind == b"acTL") {
        return None;
    }
    let args = ["-f", "png_pipe", "-i", "pipe:0", "-map_metadata", "-1", "-c:v", "png", "-pred", "mixed", "-compression_level", "9", "-f", "image2pipe", "pipe:1"];
    let encoded = ffmpeg_pipe(ffmpeg, &args, data).await?;
    if !encoded.starts_with(PNG_SIGNATURE) {
        return None;
    }
    let mut result = PNG_SIGNATURE.to_vec();
    for (kind, chunk) in png_chunks(&encoded)? {
        if is_png_color_chunk(kind) {
            continue;
        }
        result.extend_from_slice(chunk);
        // Right after the header, where they go.
        if kind == b"IHDR" {
            for (_, color) in chunks.iter().filter(|(kind, _)| is_png_color_chunk(kind)) {
                result.extend_from_slice(color);
            }
        }
    }
    Some(result)
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
