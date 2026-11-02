use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::extract::Multipart;


pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// Multipart field holding the uploaded file.
pub const IMAGE_FIELD: &str = "image";

const UPLOADS_DIR: &str = "public/uploads";
const UPLOADS_URL: &str = "/public/uploads";

/// Whose folder an uploaded image goes in.
#[derive(Clone, Copy, Debug)]
pub enum ImageOwner {
    User(i32),
    Community(i64),
}

impl ImageOwner {
    /// Folder under the uploads, like `users/12`.
    fn folder(self) -> String {
        match self {
            Self::User(id) => format!("users/{id}"),
            Self::Community(id) => format!("communities/{id}"),
        }
    }
}

/// Removes everything uploaded by `owner`: its account or community is deleted.
pub async fn remove_folder(owner: ImageOwner) {
    let path = std::path::Path::new(UPLOADS_DIR).join(owner.folder());
    match tokio::fs::remove_dir_all(&path).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => tracing::warn!("failed to remove {}: {}", path.display(), err),
    }
}

#[derive(Debug)]
pub enum ImageError {
    Missing,
    TooLarge,
    UnsupportedFormat,
    Io(std::io::Error),
}

/// Detects the format from the file signature, never from the name sent by the browser.
/// SVG is not accepted since it can contain scripts.
pub(crate) fn detect_extension(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("gif")
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("webp")
    } else if data.len() >= 12 && &data[4..12] == b"ftypavif" {
        Some("avif")
    } else {
        None
    }
}

pub(crate) async fn read_image_field(multipart: &mut Multipart) -> Result<Vec<u8>, ImageError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ImageError::Missing)?
    {
        if field.name() != Some(IMAGE_FIELD) {
            continue;
        }
        // Fails when the body goes over the global `DefaultBodyLimit`.
        let data = field.bytes().await.map_err(|_| ImageError::TooLarge)?;
        if data.is_empty() {
            return Err(ImageError::Missing);
        }
        if data.len() > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge);
        }
        return Ok(data.to_vec());
    }
    Err(ImageError::Missing)
}

/// A picture form of the cropping window (see `setupCropInputs`): the
/// picture, its `original` when it was cropped, and `recrop` when it was
/// cropped again from the original kept.
pub struct CropUpload {
    pub image: Vec<u8>,
    pub original: Option<Vec<u8>>,
    pub recrop: bool,
}

pub async fn read_crop_upload(multipart: &mut Multipart, field_name: &str) -> Result<CropUpload, ImageError> {
    let mut upload = CropUpload { image: Vec::new(), original: None, recrop: false };
    while let Some(field) = multipart.next_field().await.map_err(|_| ImageError::TooLarge)? {
        match field.name().unwrap_or_default() {
            "recrop" => upload.recrop = !field.text().await.map_err(|_| ImageError::TooLarge)?.is_empty(),
            name if name == field_name || name == "original" => {
                let is_original = name == "original";
                let data = field.bytes().await.map_err(|_| ImageError::TooLarge)?;
                if data.len() > MAX_IMAGE_BYTES {
                    return Err(ImageError::TooLarge);
                }
                if is_original {
                    upload.original = Some(data.to_vec()).filter(|data| !data.is_empty());
                } else {
                    upload.image = data.to_vec();
                }
            }
            _ => {}
        }
    }
    if upload.image.is_empty() {
        return Err(ImageError::Missing);
    }
    Ok(upload)
}

/// Saves the uploaded image in the owner's folder and returns its public URL.
/// `kind` starts the file name, like `photo`.
pub async fn save_image(owner: ImageOwner, kind: &str, multipart: &mut Multipart) -> Result<String, ImageError> {
    let data = read_image_field(multipart).await?;
    let extension = detect_extension(&data).ok_or(ImageError::UnsupportedFormat)?;
    save_file(owner, kind, extension, &data).await
}

/// Saves `data` in the owner's folder and returns its public URL. `kind`
/// starts the file name.
pub async fn save_file(owner: ImageOwner, kind: &str, extension: &str, data: &[u8]) -> Result<String, ImageError> {
    let (path, url) = new_file(owner, kind, extension).await.map_err(ImageError::Io)?;
    tokio::fs::write(path, data).await.map_err(ImageError::Io)?;
    Ok(url)
}

/// Like `save_file`, with a random name that can't be guessed: for the
/// photos and files of private messages, served to whoever has the address.
pub async fn save_private_file(owner: ImageOwner, kind: &str, extension: &str, data: &[u8]) -> Result<String, ImageError> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut random = [0u8; 16];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| ImageError::Io(std::io::Error::other("no random numbers")))?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let folder = owner.folder();
    let dir = PathBuf::from(UPLOADS_DIR).join(&folder);
    tokio::fs::create_dir_all(&dir).await.map_err(ImageError::Io)?;
    let file_name = format!("{kind}-{token}.{extension}");
    tokio::fs::write(dir.join(&file_name), data).await.map_err(ImageError::Io)?;
    Ok(format!("{UPLOADS_URL}/{folder}/{file_name}"))
}

/// The original of a picture cropped in the browser, kept to crop it again:
/// cleaned like photos (see `photo::clean`), not compressed, under a random
/// name. Never shown nor sent to other servers.
pub async fn save_original(owner: ImageOwner, data: &[u8]) -> Result<String, ImageError> {
    let photo = crate::photo::clean(data).map_err(|_| ImageError::UnsupportedFormat)?;
    save_private_file(owner, "original", photo.extension, &photo.data).await
}

/// A new file in the owner's folder, for ffmpeg to write (see `av::jobs`):
/// its path and public URL.
pub async fn new_file(owner: ImageOwner, kind: &str, extension: &str) -> std::io::Result<(PathBuf, String)> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = format!("{kind}-{nanos}.{extension}");
    let folder = owner.folder();
    let dir = PathBuf::from(UPLOADS_DIR).join(&folder);
    tokio::fs::create_dir_all(&dir).await?;
    Ok((dir.join(&file_name), format!("{UPLOADS_URL}/{folder}/{file_name}")))
}

/// Deletes a file previously returned by `save_image`.
/// URLs outside the owner's upload folder are ignored.
pub async fn remove_image(owner: ImageOwner, url: &str) {
    let folder = owner.folder();
    let prefix = format!("{UPLOADS_URL}/{folder}/");
    let Some(file_name) = url.strip_prefix(&prefix) else {
        return;
    };
    if file_name.is_empty() || file_name.contains('/') || file_name.contains("..") {
        return;
    }

    let path = PathBuf::from(UPLOADS_DIR).join(&folder).join(file_name);
    if let Err(err) = tokio::fs::remove_file(&path).await {
        tracing::warn!("failed to remove {}: {}", path.display(), err);
    }
}
