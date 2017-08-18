use std::time::SystemTime;
use std::ascii::AsciiExt;

use accept_encoding::{AcceptEncodingParser, SuffixIter};
use {AcceptEncoding};

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
    mode: Mode,
    accept_encoding: AcceptEncoding,
    range: Vec<Range>,
    if_range: Option<Result<SystemTime, ETag>>,
    if_match: Vec<ETag>,
    if_none: Vec<ETag>,
    if_unmodified: Option<SystemTime>,
    if_modified: Option<SystemTime>,
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
    pub fn suffixes(&self) -> SuffixIter {
        self.accept_encoding.suffixes()
    }
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use accept_encoding::{AcceptEncoding, AcceptEncodingParser};
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
