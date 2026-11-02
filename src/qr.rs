//! QR codes (ISO/IEC 18004), for the two-factor setup link (see `totp`):
//! bytes, error correction M (15% can be lost), versions 1 to 10 (up to 213
//! bytes). Drawn as SVG.

/// Per version (1 to 10): codewords of error correction per block, and the
/// blocks as (count, data codewords), for level M.
const BLOCKS: [(usize, [(usize, usize); 2]); 10] = [
    (10, [(1, 16), (0, 0)]),
    (16, [(1, 28), (0, 0)]),
    (26, [(1, 44), (0, 0)]),
    (18, [(2, 32), (0, 0)]),
    (24, [(2, 43), (0, 0)]),
    (16, [(4, 27), (0, 0)]),
    (18, [(4, 31), (0, 0)]),
    (22, [(2, 38), (2, 39)]),
    (22, [(3, 36), (2, 37)]),
    (26, [(4, 43), (1, 44)]),
];

/// Centers of the alignment patterns, per version.
const ALIGNMENT: [&[usize]; 10] = [&[], &[6, 18], &[6, 22], &[6, 26], &[6, 30], &[6, 34], &[6, 22, 38], &[6, 24, 42], &[6, 26, 46], &[6, 28, 50]];

fn data_capacity(version: usize) -> usize {
    BLOCKS[version - 1].1.iter().map(|(count, data)| count * data).sum()
}

/// A QR code: `size`×`size` modules, `true` is dark.
pub struct QrCode {
    pub size: usize,
    modules: Vec<bool>,
    /// Finder, timing, alignment and format modules, which masks skip.
    function: Vec<bool>,
}

impl QrCode {
    /// `None` when `data` is too long for version 10.
    pub fn encode(data: &[u8]) -> Option<Self> {
        Self::encode_with_mask(data, None)
    }

    /// With a given mask (0 to 7), else the one scoring best.
    fn encode_with_mask(data: &[u8], mask: Option<u8>) -> Option<Self> {
        let version = (1..=10).find(|version| {
            let count_bits = if *version < 10 { 8 } else { 16 };
            4 + count_bits + data.len() * 8 <= data_capacity(*version) * 8
        })?;
        let codewords = add_error_correction(&data_codewords(data, version), version);
        let mut best: Option<(u32, Self)> = None;
        for candidate in mask.map_or(0..8, |mask| mask..mask + 1) {
            let mut code = Self::empty(version);
            code.draw_function_patterns(version);
            code.draw_codewords(&codewords);
            code.apply_mask(candidate);
            code.draw_format(candidate);
            let penalty = code.penalty();
            if best.as_ref().is_none_or(|(score, _)| penalty < *score) {
                best = Some((penalty, code));
            }
        }
        best.map(|(_, code)| code)
    }

    fn empty(version: usize) -> Self {
        let size = version * 4 + 17;
        Self { size, modules: vec![false; size * size], function: vec![false; size * size] }
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }

    fn set_function(&mut self, x: usize, y: usize, dark: bool) {
        self.modules[y * self.size + x] = dark;
        self.function[y * self.size + x] = true;
    }

    fn draw_function_patterns(&mut self, version: usize) {
        let size = self.size;
        // Timing patterns.
        for index in 0..size {
            self.set_function(6, index, index % 2 == 0);
            self.set_function(index, 6, index % 2 == 0);
        }
        // Finders, with their light border.
        for (cx, cy) in [(3, 3), (size - 4, 3), (3, size - 4)] {
            for dy in -4i32..=4 {
                for dx in -4i32..=4 {
                    let (x, y) = (cx as i32 + dx, cy as i32 + dy);
                    if (0..size as i32).contains(&x) && (0..size as i32).contains(&y) {
                        let distance = dx.abs().max(dy.abs());
                        self.set_function(x as usize, y as usize, distance != 2 && distance != 4);
                    }
                }
            }
        }
        // Alignment patterns, not over the finders.
        let centers = ALIGNMENT[version - 1];
        for (row, cy) in centers.iter().enumerate() {
            for (column, cx) in centers.iter().enumerate() {
                let last = centers.len() - 1;
                if (row == 0 && column == 0) || (row == 0 && column == last) || (row == last && column == 0) {
                    continue;
                }
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        self.set_function((*cx as i32 + dx) as usize, (*cy as i32 + dy) as usize, dx.abs().max(dy.abs()) != 1);
                    }
                }
            }
        }
        // Reserved now, drawn by `draw_format`.
        self.draw_format(0);
        // Version information, from version 7.
        if version >= 7 {
            let mut remainder = version as u32;
            for _ in 0..12 {
                remainder = (remainder << 1) ^ ((remainder >> 11) * 0x1F25);
            }
            let bits = (version as u32) << 12 | remainder;
            for index in 0..18 {
                let dark = (bits >> index) & 1 == 1;
                let (a, b) = (size - 11 + index % 3, index / 3);
                self.set_function(a, b, dark);
                self.set_function(b, a, dark);
            }
        }
    }

    /// The level (M) and mask, twice, with their error correction.
    fn draw_format(&mut self, mask: u8) {
        let size = self.size;
        // Level M is 00.
        let data = u32::from(mask);
        let mut remainder = data;
        for _ in 0..10 {
            remainder = (remainder << 1) ^ ((remainder >> 9) * 0x537);
        }
        let bits = (data << 10 | remainder) ^ 0x5412;
        let bit = |index: usize| (bits >> index) & 1 == 1;
        for index in 0..=5 {
            self.set_function(8, index, bit(index));
        }
        self.set_function(8, 7, bit(6));
        self.set_function(8, 8, bit(7));
        self.set_function(7, 8, bit(8));
        for index in 9..15 {
            self.set_function(14 - index, 8, bit(index));
        }
        for index in 0..8 {
            self.set_function(size - 1 - index, 8, bit(index));
        }
        for index in 8..15 {
            self.set_function(8, size - 15 + index, bit(index));
        }
        // Always dark.
        self.set_function(8, size - 8, true);
    }

    /// In two-column zigzags from the bottom right, skipping column 6.
    fn draw_codewords(&mut self, codewords: &[u8]) {
        let size = self.size;
        let mut bit = 0;
        let mut right = size as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vertical in 0..size {
                for column in 0..2 {
                    let x = right as usize - column;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { size - 1 - vertical } else { vertical };
                    if !self.function[y * size + x] && bit < codewords.len() * 8 {
                        self.modules[y * size + x] = (codewords[bit / 8] >> (7 - bit % 8)) & 1 == 1;
                        bit += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let flip = match mask {
                    0 => (x + y) % 2 == 0,
                    1 => y % 2 == 0,
                    2 => x % 3 == 0,
                    3 => (x + y) % 3 == 0,
                    4 => (x / 3 + y / 2) % 2 == 0,
                    5 => x * y % 2 + x * y % 3 == 0,
                    6 => (x * y % 2 + x * y % 3) % 2 == 0,
                    _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
                };
                if flip && !self.function[y * self.size + x] {
                    self.modules[y * self.size + x] ^= true;
                }
            }
        }
    }

    /// How hard it is to read (lower is better): long runs, 2×2 blocks,
    /// patterns looking like finders, too much dark or light.
    fn penalty(&self) -> u32 {
        let size = self.size;
        let mut score = 0;
        let lines = |vertical: bool| -> Vec<Vec<bool>> {
            (0..size).map(|a| (0..size).map(|b| if vertical { self.get(a, b) } else { self.get(b, a) }).collect()).collect()
        };
        let finder_like = [true, false, true, true, true, false, true];
        for line in lines(false).into_iter().chain(lines(true)) {
            let mut run = 1;
            for index in 1..=size {
                if index < size && line[index] == line[index - 1] {
                    run += 1;
                    continue;
                }
                if run >= 5 {
                    score += 3 + (run - 5) as u32;
                }
                run = 1;
            }
            for start in 0..size.saturating_sub(6) {
                if line[start..start + 7] == finder_like {
                    let light = |from: usize, to: usize| (from..to).all(|index| index >= size || !line[index]);
                    let before = start >= 4 && light(start - 4, start);
                    if before || light(start + 7, start + 11) {
                        score += 40;
                    }
                }
            }
        }
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let color = self.get(x, y);
                if color == self.get(x + 1, y) && color == self.get(x, y + 1) && color == self.get(x + 1, y + 1) {
                    score += 3;
                }
            }
        }
        let dark = self.modules.iter().filter(|dark| **dark).count();
        let percent = dark * 100 / self.modules.len();
        score + (percent.abs_diff(50) / 5) as u32 * 10
    }

    /// Black squares on white, with the 4-module quiet zone readers need.
    pub fn to_svg(&self) -> String {
        let full = self.size + 8;
        let mut path = String::new();
        for y in 0..self.size {
            for x in 0..self.size {
                if self.get(x, y) {
                    path.push_str(&format!("M{} {}h1v1h-1z", x + 4, y + 4));
                }
            }
        }
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {full} {full}\" shape-rendering=\"crispEdges\"><rect width=\"{full}\" height=\"{full}\" fill=\"#fff\"/><path d=\"{path}\" fill=\"#000\"/></svg>"
        )
    }
}

/// Mode (bytes), length, the bytes, an end mark, and padding.
fn data_codewords(data: &[u8], version: usize) -> Vec<u8> {
    let capacity = data_capacity(version);
    fn push(bits: &mut Vec<bool>, value: u32, count: usize) {
        for index in (0..count).rev() {
            bits.push((value >> index) & 1 == 1);
        }
    }
    let mut bits: Vec<bool> = Vec::with_capacity(capacity * 8);
    push(&mut bits, 0b0100, 4);
    push(&mut bits, data.len() as u32, if version < 10 { 8 } else { 16 });
    for byte in data {
        push(&mut bits, u32::from(*byte), 8);
    }
    let terminator = (capacity * 8 - bits.len()).min(4);
    push(&mut bits, 0, terminator);
    let padding = (8 - bits.len() % 8) % 8;
    push(&mut bits, 0, padding);
    let mut codewords: Vec<u8> = bits.chunks(8).map(|byte| byte.iter().fold(0, |value, bit| (value << 1) | u8::from(*bit))).collect();
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if codewords.len() >= capacity {
            break;
        }
        codewords.push(pad);
    }
    codewords
}

/// Split in blocks, each with its Reed-Solomon codewords, then interleaved.
fn add_error_correction(data: &[u8], version: usize) -> Vec<u8> {
    let (ec_length, groups) = BLOCKS[version - 1];
    let generator = rs_generator(ec_length);
    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut start = 0;
    for (count, length) in groups {
        for _ in 0..count {
            blocks.push(&data[start..start + length]);
            start += length;
        }
    }
    let corrections: Vec<Vec<u8>> = blocks.iter().map(|block| rs_remainder(block, &generator)).collect();
    let longest = blocks.iter().map(|block| block.len()).max().unwrap_or(0);
    let mut result = Vec::with_capacity(data.len() + ec_length * blocks.len());
    for index in 0..longest {
        for block in &blocks {
            if index < block.len() {
                result.push(block[index]);
            }
        }
    }
    for index in 0..ec_length {
        for correction in &corrections {
            result.push(correction[index]);
        }
    }
    result
}

/// Multiplication in GF(256) with the polynomial of QR codes (0x11D).
fn gf_multiply(mut a: u8, mut b: u8) -> u8 {
    let mut product = 0u8;
    while b != 0 {
        if b & 1 == 1 {
            product ^= a;
        }
        let carry = a & 0x80 != 0;
        a <<= 1;
        if carry {
            a ^= 0x1D;
        }
        b >>= 1;
    }
    product
}

/// (x - α⁰)(x - α¹)…(x - α^(degree-1)), its coefficients after the leading 1.
fn rs_generator(degree: usize) -> Vec<u8> {
    let mut coefficients = vec![0u8; degree];
    coefficients[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for index in 0..degree {
            coefficients[index] = gf_multiply(coefficients[index], root);
            if index + 1 < degree {
                coefficients[index] ^= coefficients[index + 1];
            }
        }
        root = gf_multiply(root, 2);
    }
    coefficients
}

fn rs_remainder(data: &[u8], generator: &[u8]) -> Vec<u8> {
    let mut remainder = vec![0u8; generator.len()];
    for byte in data {
        let factor = byte ^ remainder.remove(0);
        remainder.push(0);
        for (value, coefficient) in remainder.iter_mut().zip(generator) {
            *value ^= gf_multiply(*coefficient, factor);
        }
    }
    remainder
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `qrencode -l M -8 -t ASCII`'s matrix and its mask, if installed.
    fn reference(text: &str) -> Option<(Vec<Vec<bool>>, u8)> {
        let output = std::process::Command::new("qrencode").args(["-l", "M", "-8", "-m", "0", "-t", "ASCII", text]).output().ok()?;
        let rows: Vec<Vec<bool>> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.as_bytes().chunks(2).map(|cell| cell[0] == b'#').collect())
            .collect();
        // The mask, from the 15 format bits beside the top left finder
        // (placed as `draw_format` does): bits 10 to 12.
        let mut places: Vec<(usize, usize)> = (0..=5).map(|index| (8, index)).collect();
        places.extend([(8, 7), (8, 8), (7, 8)]);
        places.extend((9..15).map(|index| (14 - index, 8)));
        let bits = places.iter().enumerate().fold(0u32, |bits, (index, (x, y))| bits | u32::from(rows[*y][*x]) << index);
        let mask = (((bits ^ 0x5412) >> 10) & 7) as u8;
        Some((rows, mask))
    }

    #[test]
    fn same_as_qrencode() {
        let texts = [
            "hi",
            "otpauth://totp/Tison:alice?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=Tison",
            "otpauth://totp/tison.example.org:a_rather_long_username_here?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=tison.example.org",
        ];
        for text in texts {
            let Some((expected, mask)) = reference(text) else {
                eprintln!("qrencode not installed: skipped");
                return;
            };
            let code = QrCode::encode_with_mask(text.as_bytes(), Some(mask)).unwrap();
            assert_eq!(code.size, expected.len(), "size of {text}");
            for (y, row) in expected.iter().enumerate() {
                for (x, dark) in row.iter().enumerate() {
                    assert_eq!(code.get(x, y), *dark, "module ({x}, {y}) of {text}");
                }
            }
        }
    }

    #[test]
    fn too_long_is_refused() {
        assert!(QrCode::encode(&[b'a'; 213]).is_some());
        assert!(QrCode::encode(&[b'a'; 214]).is_none());
    }
}
