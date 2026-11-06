//! ZIP archives written by hand, for the export of a user's data (see
//! `handlers::export`). Files are stored as they are (photos and videos are
//! already compressed), always in ZIP64 so an export over 4 GB opens too:
//! Windows, macOS and Linux all read it. Names are UTF-8.

use std::io::{self, Read, Write};

/// CRC-32 (IEEE), the checksum of each file.
fn crc32_table() -> &'static [u32; 256] {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let mut value = index as u32;
            for _ in 0..8 {
                value = if value & 1 == 1 { 0xEDB8_8320 ^ (value >> 1) } else { value >> 1 };
            }
            *entry = value;
        }
        table
    })
}

#[derive(Clone, Copy)]
pub struct Crc32(u32);

impl Crc32 {
    pub fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    pub fn update(&mut self, data: &[u8]) {
        let table = crc32_table();
        for byte in data {
            self.0 = table[((self.0 ^ u32::from(*byte)) & 0xFF) as usize] ^ (self.0 >> 8);
        }
    }

    pub fn value(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

struct Entry {
    name: String,
    crc: u32,
    size: u64,
    offset: u64,
}

/// An archive being written to `out`, entry after entry, then `finish`.
pub struct ZipWriter<W: Write> {
    out: W,
    written: u64,
    entries: Vec<Entry>,
    /// MS-DOS time and date of every entry: when the archive was made.
    time: u16,
    date: u16,
}

impl<W: Write> ZipWriter<W> {
    /// `when`: year, month, day, hour, minute, second (UTC).
    pub fn new(out: W, when: (u16, u16, u16, u16, u16, u16)) -> Self {
        let (year, month, day, hour, minute, second) = when;
        Self {
            out,
            written: 0,
            entries: Vec::new(),
            time: (hour << 11) | (minute << 5) | (second / 2),
            date: (year.saturating_sub(1980) << 9) | (month << 5) | day,
        }
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write_all(data)?;
        self.written += data.len() as u64;
        Ok(())
    }

    fn local_header(&mut self, name: &str, crc: u32, size: u64) -> io::Result<()> {
        let mut header = Vec::with_capacity(50 + name.len());
        header.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        header.extend_from_slice(&45u16.to_le_bytes()); // ZIP64
        header.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        header.extend_from_slice(&0u16.to_le_bytes()); // stored
        header.extend_from_slice(&self.time.to_le_bytes());
        header.extend_from_slice(&self.date.to_le_bytes());
        header.extend_from_slice(&crc.to_le_bytes());
        header.extend_from_slice(&u32::MAX.to_le_bytes());
        header.extend_from_slice(&u32::MAX.to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&20u16.to_le_bytes());
        header.extend_from_slice(name.as_bytes());
        // ZIP64 extra field: the sizes.
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        self.write(&header)
    }

    /// Adds `data` as the file `name` (folders with `/`).
    pub fn add_bytes(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let mut crc = Crc32::new();
        crc.update(data);
        let offset = self.written;
        self.local_header(name, crc.value(), data.len() as u64)?;
        self.write(data)?;
        self.entries.push(Entry { name: name.to_string(), crc: crc.value(), size: data.len() as u64, offset });
        Ok(())
    }

    /// Adds the file at `path` as `name`, read twice (its checksum comes
    /// first): never all in memory, a video may be large.
    pub fn add_file(&mut self, name: &str, path: &std::path::Path) -> io::Result<()> {
        let mut crc = Crc32::new();
        let mut size = 0u64;
        let mut buffer = vec![0u8; 256 * 1024];
        let mut file = std::fs::File::open(path)?;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            crc.update(&buffer[..read]);
            size += read as u64;
        }
        let offset = self.written;
        self.local_header(name, crc.value(), size)?;
        let mut file = std::fs::File::open(path)?;
        let mut copied = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            self.write(&buffer[..read])?;
            copied += read as u64;
        }
        if copied != size {
            return Err(io::Error::other(format!("{} changed while it was added", path.display())));
        }
        self.entries.push(Entry { name: name.to_string(), crc: crc.value(), size, offset });
        Ok(())
    }

    /// Writes the directory of the entries: the archive is complete.
    pub fn finish(mut self) -> io::Result<W> {
        let directory_offset = self.written;
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            let mut header = Vec::with_capacity(74 + entry.name.len());
            header.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            header.extend_from_slice(&45u16.to_le_bytes());
            header.extend_from_slice(&45u16.to_le_bytes());
            header.extend_from_slice(&0x0800u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&self.time.to_le_bytes());
            header.extend_from_slice(&self.date.to_le_bytes());
            header.extend_from_slice(&entry.crc.to_le_bytes());
            header.extend_from_slice(&u32::MAX.to_le_bytes());
            header.extend_from_slice(&u32::MAX.to_le_bytes());
            header.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            header.extend_from_slice(&28u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes()); // comment
            header.extend_from_slice(&0u16.to_le_bytes()); // disk
            header.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            header.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            header.extend_from_slice(&u32::MAX.to_le_bytes());
            header.extend_from_slice(entry.name.as_bytes());
            header.extend_from_slice(&1u16.to_le_bytes());
            header.extend_from_slice(&24u16.to_le_bytes());
            header.extend_from_slice(&entry.size.to_le_bytes());
            header.extend_from_slice(&entry.size.to_le_bytes());
            header.extend_from_slice(&entry.offset.to_le_bytes());
            self.write(&header)?;
        }
        let directory_size = self.written - directory_offset;
        let end64_offset = self.written;
        let count = entries.len() as u64;
        let mut end = Vec::with_capacity(98);
        // ZIP64 end of central directory, then where it is.
        end.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        end.extend_from_slice(&44u64.to_le_bytes());
        end.extend_from_slice(&45u16.to_le_bytes());
        end.extend_from_slice(&45u16.to_le_bytes());
        end.extend_from_slice(&0u32.to_le_bytes());
        end.extend_from_slice(&0u32.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&directory_size.to_le_bytes());
        end.extend_from_slice(&directory_offset.to_le_bytes());
        end.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        end.extend_from_slice(&0u32.to_le_bytes());
        end.extend_from_slice(&end64_offset.to_le_bytes());
        end.extend_from_slice(&1u32.to_le_bytes());
        // The classic end, pointing to the ZIP64 one.
        end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&u16::MAX.to_le_bytes());
        end.extend_from_slice(&u16::MAX.to_le_bytes());
        end.extend_from_slice(&u32::MAX.to_le_bytes());
        end.extend_from_slice(&u32::MAX.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.write(&end)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_is_the_standard_one() {
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(crc.value(), 0xCBF4_3926);
    }

    #[test]
    fn archives_are_complete() {
        let mut zip = ZipWriter::new(Vec::new(), (2026, 11, 4, 12, 30, 10));
        zip.add_bytes("profile.json", b"{}").unwrap();
        zip.add_bytes("média/é.txt", b"hello").unwrap();
        let data = zip.finish().unwrap();
        assert!(data.starts_with(&0x0403_4b50u32.to_le_bytes()));
        // Ends with the classic end record, the entries counted in ZIP64.
        assert_eq!(&data[data.len() - 22..data.len() - 18], &0x0605_4b50u32.to_le_bytes());
        assert!(data.windows(5).any(|window| window == b"hello"));
    }
}
