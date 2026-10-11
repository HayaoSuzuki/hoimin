pub struct SourceEncoding {
    pub name: &'static str,
    pub cookie: &'static str,
    pub prefix: &'static [u8],
    pub encode: fn(&str) -> Vec<u8>,
}

pub const ENCODINGS: [SourceEncoding; 3] = [
    SourceEncoding {
        name: "utf8",
        cookie: "utf-8",
        prefix: b"",
        encode: |text| text.as_bytes().to_vec(),
    },
    SourceEncoding {
        name: "bom",
        cookie: "utf-8",
        prefix: &[0xef, 0xbb, 0xbf],
        encode: |text| text.as_bytes().to_vec(),
    },
    SourceEncoding {
        name: "latin1",
        cookie: "latin-1",
        prefix: b"",
        encode: |text| {
            text.chars()
                .map(|character| u8::try_from(u32::from(character)).unwrap())
                .collect()
        },
    },
];

impl SourceEncoding {
    pub fn source(&self, text: &str) -> Vec<u8> {
        [self.prefix, (self.encode)(text).as_slice()].concat()
    }
}
