use crate::{Result, canonical::array};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const COLOURS: [(&str, [u8; 3]); 4] = [
    ("red", [230, 20, 20]),
    ("green", [20, 190, 20]),
    ("blue", [20, 20, 230]),
    ("yellow", [230, 230, 20]),
];

/// The answer lives only in native memory. Neither text nor filenames reveal it.
pub(crate) struct ImageProbe {
    expected: Value,
    images: Vec<String>,
    digests: Vec<String>,
    index: usize,
    observed: Vec<Value>,
}
impl ImageProbe {
    pub(crate) fn new() -> Self {
        let mut expected = Vec::new();
        let mut images = Vec::new();
        let mut digests = Vec::new();
        for _ in 0..2 {
            let mut order = [0, 1, 2, 3];
            let entropy = uuid::Uuid::new_v4();
            for i in (1..4).rev() {
                order.swap(i, usize::from(entropy.as_bytes()[i]) % (i + 1));
            }
            let bytes = png(&order);
            expected.push(
                order
                    .iter()
                    .map(|index| COLOURS[*index].0)
                    .collect::<Vec<_>>(),
            );
            digests.push(format!("{:x}", Sha256::digest(&bytes)));
            images.push(format!("data:image/png;base64,{}", base64(&bytes)));
        }
        Self {
            expected: json!(expected),
            images,
            digests,
            index: 0,
            observed: Vec::new(),
        }
    }
    pub(crate) fn request(&self, connection: &Value) -> Value {
        let content = vec![
            json!({"type":"input_text","text":"Inspect the attached image. It has four coloured squares. Call engine_image_probe once with the colours in top-left, top-right, bottom-left, bottom-right order. Use only red, green, blue or yellow. Do not infer from this text."}),
            json!({"type":"input_image","image_url":self.images[self.index],"detail":"low"}),
        ];
        json!({"model":connection["id"],"instructions":"Synthetic side-effect-free image input qualification. Read the actual image pixels; no user data is included.","input":[{"type":"message","role":"user","content":content}],"tools":[{"type":"function","name":"engine_image_probe","description":"Report observed colours from the attached image.","parameters":{"type":"object","properties":{"colours":{"type":"array","minItems":4,"maxItems":4,"items":{"type":"string","enum":["red","green","blue","yellow"]}}},"required":["colours"],"additionalProperties":false},"strict":false}],"tool_choice":"required","stream":false,"max_output_tokens":384})
    }
    pub(crate) fn validate(&mut self, canonical: &Value) -> Result<Option<Value>> {
        let output = array(&canonical["output"]);
        let calls: Vec<_> = output
            .iter()
            .filter(|item| item["type"] == "function_call" || item["type"] == "custom_tool_call")
            .collect();
        if calls.len() != 1
            || calls[0]["type"] != "function_call"
            || calls[0]["name"] != "engine_image_probe"
        {
            return Err("image probe must return exactly one engine_image_probe tool call".into());
        }
        let observed: Value = serde_json::from_str(
            calls[0]["arguments"]
                .as_str()
                .ok_or("image probe arguments are required")?,
        )
        .map_err(|_| "image probe arguments are not valid JSON")?;
        if observed != json!({"colours":self.expected[self.index]}) {
            return Err("image probe did not correctly identify the image pixels".into());
        }
        self.observed.push(observed);
        self.index += 1;
        Ok((self.index == self.images.len()).then(||json!({"basis":"randomized-pixel-recognition","imageDigests":self.digests,"imageCount":2,"observed":self.observed})))
    }
}

// Bounded, dependency-free PNG encoder: opaque RGB, uncompressed zlib blocks.
fn png(order: &[usize; 4]) -> Vec<u8> {
    let mut rows = Vec::with_capacity(128 * 385);
    for y in 0..128 {
        rows.push(0);
        for x in 0..128 {
            let index = (y / 64) * 2 + x / 64;
            rows.extend_from_slice(if x % 64 < 4 || x % 64 > 59 || y % 64 < 4 || y % 64 > 59 {
                &[255, 255, 255]
            } else {
                &COLOURS[order[index]].1
            });
        }
    }
    let mut compressed = vec![0x78, 0x01, 1];
    let length = rows.len() as u16;
    compressed.extend_from_slice(&length.to_le_bytes());
    compressed.extend_from_slice(&(!length).to_le_bytes());
    compressed.extend_from_slice(&rows);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &rows {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    compressed.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::from(128u32.to_be_bytes());
    header.extend_from_slice(&128u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &compressed);
    chunk(&mut png, b"IEND", &[]);
    png
}
fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], content: &[u8]) {
    output.extend_from_slice(&(content.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(content);
    let mut crc = !0u32;
    for byte in kind.iter().chain(content) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    output.extend_from_slice(&(!crc).to_be_bytes());
}
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for part in bytes.chunks(3) {
        let n = u32::from(part[0]) << 16
            | u32::from(*part.get(1).unwrap_or(&0)) << 8
            | u32::from(*part.get(2).unwrap_or(&0));
        output.push(ALPHABET[(n >> 18) as usize] as char);
        output.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        output.push(if part.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        output.push(if part.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}
