use crate::Result;
use serde_json::{Value, json};

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 20 * 1024 * 1024;
// Reservation, not billed usage. Actual provider usage settles the existing budget.
// 64K conservatively covers patch/tile vision models for the admitted 2048px bound.
const RESERVED_TOKENS_PER_IMAGE: usize = 65_536;

fn decode(input: &str) -> Result<Vec<u8>> {
    if input.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
        return Err("image exceeds 8MiB byte limit".into());
    }
    if input.len() % 4 != 0 {
        return Err("invalid image base64 length".into());
    }
    let pad = input.bytes().rev().take_while(|b| *b == b'=').count();
    if pad > 2 {
        return Err("invalid image base64 padding".into());
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    let mut padding = false;
    for b in input.bytes() {
        if b == b'=' {
            padding = true;
            continue;
        }
        if padding {
            return Err("invalid image base64 padding".into());
        }
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("invalid image base64".into()),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    if bits != pad * 2 || acc & ((1u32 << bits) - 1) != 0 {
        return Err("invalid image base64 padding bits".into());
    }
    if out.is_empty() || out.len() > MAX_IMAGE_BYTES {
        return Err("invalid image byte size".into());
    }
    Ok(out)
}
fn dimensions(b: &[u8]) -> Result<(u32, u32)> {
    if b.len() >= 24 && b.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut i = 8;
        while i + 12 <= b.len() {
            let n = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            if n > b.len() - i - 12 {
                break;
            }
            if &b[i + 4..i + 8] == b"acTL" {
                return Err("animated images are outside the admitted vision bound".into());
            }
            i += n + 12;
        }
        return Ok((
            u32::from_be_bytes(b[16..20].try_into().unwrap()),
            u32::from_be_bytes(b[20..24].try_into().unwrap()),
        ));
    }
    if b.len() >= 30 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        let n = |s: &[u8]| u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16;
        if &b[12..16] == b"VP8X" {
            if b[20] & 2 != 0 {
                return Err("animated images are outside the admitted vision bound".into());
            }
            return Ok((1 + n(&b[24..27]), 1 + n(&b[27..30])));
        }
        if &b[12..16] == b"VP8 " && &b[23..26] == b"\x9d\x01\x2a" {
            return Ok((
                u32::from(u16::from_le_bytes(b[26..28].try_into().unwrap()) & 0x3fff),
                u32::from(u16::from_le_bytes(b[28..30].try_into().unwrap()) & 0x3fff),
            ));
        }
        if &b[12..16] == b"VP8L" && b[20] == 0x2f {
            let x = u32::from_le_bytes(b[21..25].try_into().unwrap());
            return Ok((1 + (x & 0x3fff), 1 + ((x >> 14) & 0x3fff)));
        }
    }
    if b.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        while i + 4 < b.len() {
            if b[i] != 0xff {
                break;
            }
            let marker = b[i + 1];
            i += 2;
            if marker == 0xff || marker == 0x00 {
                continue;
            }
            let size = usize::from(u16::from_be_bytes([b[i], b[i + 1]]));
            if size < 2 || i + size > b.len() {
                break;
            }
            if matches!(
                marker,
                0xc0 | 0xc1
                    | 0xc2
                    | 0xc3
                    | 0xc5
                    | 0xc6
                    | 0xc7
                    | 0xc9
                    | 0xca
                    | 0xcb
                    | 0xcd
                    | 0xce
                    | 0xcf
            ) && size >= 7
            {
                return Ok((
                    u32::from(u16::from_be_bytes([b[i + 5], b[i + 6]])),
                    u32::from(u16::from_be_bytes([b[i + 3], b[i + 4]])),
                ));
            }
            i += size;
        }
    }
    Err("image format/dimensions are unsupported".into())
}
pub(crate) fn admission(input: &Value) -> Result<(Value, usize)> {
    let mut sanitized = input.clone();
    let mut count = 0;
    let mut bytes = 0;
    if let Some(items) = sanitized.as_array_mut() {
        for item in items {
            if let Some(parts) = item["content"].as_array_mut() {
                for part in parts {
                    if part["type"] != "input_image" {
                        continue;
                    }
                    count += 1;
                    if count > 5 {
                        return Err("at most five input images are admitted".into());
                    }
                    let url = part["image_url"]
                        .as_str()
                        .ok_or("image_url must be an embedded data URL")?;
                    let (prefix, encoded) =
                        url.split_once(",").ok_or("image data URL is invalid")?;
                    if !matches!(
                        prefix,
                        "data:image/png;base64"
                            | "data:image/jpeg;base64"
                            | "data:image/webp;base64"
                    ) {
                        return Err("only embedded static PNG/JPEG/WebP images are admitted".into());
                    }
                    if !matches!(
                        part["detail"].as_str(),
                        None | Some("high" | "auto" | "low")
                    ) {
                        return Err(
                            "original image detail is outside the admitted input bound".into()
                        );
                    }
                    let image = decode(encoded)?;
                    bytes += image.len();
                    if bytes > MAX_TOTAL_BYTES {
                        return Err("input images exceed 20MiB total".into());
                    }
                    let (w, h) = dimensions(&image)?;
                    if w == 0 || h == 0 || w > 2048 || h > 2048 {
                        return Err("input images must fit within 2048x2048 pixels".into());
                    }
                    part["image_url"] =
                        json!(format!("[embedded raster {w}x{h} {} bytes]", image.len()));
                }
            }
        }
    }
    Ok((sanitized, count * RESERVED_TOKENS_PER_IMAGE))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_remote_images() {
        assert!(admission(&json!([{"type":"message","content":[{"type":"input_image","image_url":"http://127.0.0.1/private"}]}])).is_err());
    }
    #[test]
    fn text_unaffected() {
        let input = json!([{"type":"message","content":[{"type":"input_text","text":"hello"}]}]);
        assert_eq!(admission(&input).unwrap(), (input, 0));
    }
    #[test]
    fn reserves_vision_separately_and_preserves_original_bytes() {
        let url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jfQAAAABJRU5ErkJggg==";
        let input = json!([{"type":"message","content":[{"type":"input_image","image_url":url}]}]);
        let (sanitized, tokens) = admission(&input).unwrap();
        assert_eq!(tokens, 65_536);
        assert!(sanitized.to_string().contains("raster 1x1"));
        assert_eq!(input[0]["content"][0]["image_url"], url);
        let too_many = Value::Array(vec![input[0].clone(); 6]);
        assert!(admission(&too_many).is_err());
    }
    #[test]
    fn rejects_malformed_padding() {
        for value in ["AAA", "AAAA=", "AAAA====", "AR==", "AAB="] {
            assert!(decode(value).is_err(), "{value}");
        }
        assert_eq!(decode("AQ==").unwrap(), vec![1]);
    }
}
