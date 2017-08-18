use std::time::SystemTime;
use std::ascii::AsciiExt;
use std::fs::Metadata;

use accept_encoding::{AcceptEncodingParser, Iter as EncodingIter};
use {AcceptEncoding, Encoding, Output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Head,
    Get,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
pub enum Range {
    FromTo(u64, u64),
    AllFrom(u64),
    Last(u64),
}

#[derive(Clone, Debug)]
pub struct ETag {
}

#[derive(Debug, Clone)]
pub struct Input {
    pub(crate) mode: Mode,
    pub(crate) accept_encoding: AcceptEncoding,
    pub(crate) range: Vec<Range>,
    pub(crate) if_range: Option<Result<SystemTime, ETag>>,
    pub(crate) if_match: Vec<ETag>,
    pub(crate) if_none: Vec<ETag>,
    pub(crate) if_unmodified: Option<SystemTime>,
    pub(crate) if_modified: Option<SystemTime>,
}

impl Input {
    pub fn from_headers<'x, I>(method: &str, headers: I) -> Input
        where I: Iterator<Item=(&'x str, &'x[u8])>
    {
        let mode = match method {
            "HEAD" => Mode::Head,
            "GET" => Mode::Get,
            _ => return Input {
                mode: Mode::Invalid,
                accept_encoding: AcceptEncoding::identity(),
                range: Vec::new(),
                if_range: None,
                if_match: Vec::new(),
                if_none: Vec::new(),
                if_unmodified: None,
                if_modified: None,
            },
        };
        let mut ae_parser = AcceptEncodingParser::new();
        for (key, val) in headers {
            if key.eq_ignore_ascii_case("accept-encoding") {
                ae_parser.add_header(val);
            }
        }
        Input {
            mode: mode,
            accept_encoding: ae_parser.done(),
            range: Vec::new(),
            if_range: None,
            if_match: Vec::new(),
            if_none: Vec::new(),
            if_unmodified: None,
            if_modified: None,
        }
    }
    pub fn encodings(&self) -> EncodingIter {
        self.accept_encoding.iter()
    }
    pub fn prepare_file(&self, encoding: Encoding, metadata: &Metadata)
        -> Output
    {
        Output::from_file(self, encoding, metadata)
    }
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use accept_encoding::{AcceptEncodingParser};
    use super::*;

    fn send<T: Send>(_: &T) {}
    fn self_contained<T: 'static>(_: &T) {}

    #[test]
    fn traits() {
        let v = Input {
            mode: Mode::Get,
            accept_encoding: AcceptEncodingParser::new().done(),
            range: Vec::new(),
            if_range: None,
            if_match: Vec::new(),
            if_none: Vec::new(),
            if_unmodified: None,
            if_modified: None,
        };
        send(&v);
        self_contained(&v);
    }

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert_eq!(size_of::<Range>(), 24);
        assert_eq!(size_of::<Input>(), 160);
    }
}
