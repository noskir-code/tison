//! AMF0, how RTMP writes its commands (`connect`, `publish`...): numbers,
//! booleans, strings, objects, null. Enough for OBS and ffmpeg.

#[derive(Clone, Debug, PartialEq)]
pub enum Amf {
    Number(f64),
    Boolean(bool),
    String(String),
    Object(Vec<(String, Amf)>),
    Null,
    /// `undefined`, dates, and what isn't read.
    Undefined,
}

impl Amf {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(*number),
            _ => None,
        }
    }
}

/// Every value of a message.
pub fn decode_all(mut data: &[u8]) -> Result<Vec<Amf>, String> {
    let mut values = Vec::new();
    while !data.is_empty() {
        values.push(decode(&mut data)?);
    }
    Ok(values)
}

fn take<'a>(data: &mut &'a [u8], count: usize) -> Result<&'a [u8], String> {
    if data.len() < count {
        return Err("AMF value cut".to_string());
    }
    let (taken, rest) = data.split_at(count);
    *data = rest;
    Ok(taken)
}

fn decode_string(data: &mut &[u8], long: bool) -> Result<String, String> {
    let length = if long {
        u32::from_be_bytes(take(data, 4)?.try_into().unwrap_or_default()) as usize
    } else {
        u16::from_be_bytes(take(data, 2)?.try_into().unwrap_or_default()) as usize
    };
    Ok(String::from_utf8_lossy(take(data, length)?).into_owned())
}

/// Keys and values up to the empty key followed by the end mark (9).
fn decode_properties(data: &mut &[u8]) -> Result<Vec<(String, Amf)>, String> {
    let mut properties = Vec::new();
    loop {
        let key = decode_string(data, false)?;
        if key.is_empty() && data.first() == Some(&9) {
            *data = &data[1..];
            return Ok(properties);
        }
        properties.push((key, decode(data)?));
    }
}

fn decode(data: &mut &[u8]) -> Result<Amf, String> {
    let marker = take(data, 1)?[0];
    Ok(match marker {
        0 => Amf::Number(f64::from_be_bytes(take(data, 8)?.try_into().unwrap_or_default())),
        1 => Amf::Boolean(take(data, 1)?[0] != 0),
        2 => Amf::String(decode_string(data, false)?),
        3 => Amf::Object(decode_properties(data)?),
        5 => Amf::Null,
        6 => Amf::Undefined,
        // ECMA array: a count, then like an object.
        8 => {
            take(data, 4)?;
            Amf::Object(decode_properties(data)?)
        }
        // Strict array: its values are read, not kept.
        10 => {
            let count = u32::from_be_bytes(take(data, 4)?.try_into().unwrap_or_default());
            for _ in 0..count {
                decode(data)?;
            }
            Amf::Undefined
        }
        // Date: milliseconds and a time zone.
        11 => {
            take(data, 10)?;
            Amf::Undefined
        }
        12 => Amf::String(decode_string(data, true)?),
        other => return Err(format!("unknown AMF value {other}")),
    })
}

pub fn encode(values: &[Amf]) -> Vec<u8> {
    let mut data = Vec::new();
    for value in values {
        encode_value(value, &mut data);
    }
    data
}

fn encode_string(text: &str, data: &mut Vec<u8>) {
    let bytes = &text.as_bytes()[..text.len().min(u16::MAX as usize)];
    data.extend((bytes.len() as u16).to_be_bytes());
    data.extend(bytes);
}

fn encode_value(value: &Amf, data: &mut Vec<u8>) {
    match value {
        Amf::Number(number) => {
            data.push(0);
            data.extend(number.to_be_bytes());
        }
        Amf::Boolean(flag) => data.extend([1, u8::from(*flag)]),
        Amf::String(text) => {
            data.push(2);
            encode_string(text, data);
        }
        Amf::Object(properties) => {
            data.push(3);
            for (key, value) in properties {
                encode_string(key, data);
                encode_value(value, data);
            }
            data.extend([0, 0, 9]);
        }
        Amf::Null => data.push(5),
        Amf::Undefined => data.push(6),
    }
}

/// An object, from `("key", value)` pairs.
pub fn object(properties: &[(&str, Amf)]) -> Amf {
    Amf::Object(properties.iter().map(|(key, value)| (key.to_string(), value.clone())).collect())
}

pub fn text(value: &str) -> Amf {
    Amf::String(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let values = vec![
            text("connect"),
            Amf::Number(1.0),
            object(&[("app", text("live")), ("fpad", Amf::Boolean(false))]),
            Amf::Null,
        ];
        assert_eq!(decode_all(&encode(&values)).unwrap(), values);
    }

    #[test]
    fn ecma_arrays_and_cut_data() {
        // onMetaData as OBS sends it: an ECMA array.
        let mut data = vec![8, 0, 0, 0, 1];
        data.extend([0, 5]);
        data.extend(b"width");
        data.push(0);
        data.extend(1280f64.to_be_bytes());
        data.extend([0, 0, 9]);
        assert_eq!(decode_all(&data).unwrap(), vec![object(&[("width", Amf::Number(1280.0))])]);
        assert!(decode_all(&[2, 0, 10, b'a']).is_err());
    }
}
