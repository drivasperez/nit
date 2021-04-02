use std::borrow::Cow;

use super::Object;

pub struct Blob {
    data: Vec<u8>,
}

impl Blob {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn to_bytestr(&self) -> &[u8] {
        &self.data
    }
}

impl Object for Blob {
    fn data(&self) -> Cow<[u8]> {
        Cow::Borrowed(self.to_bytestr())
    }

    fn kind(&self) -> &str {
        "blob"
    }
}
