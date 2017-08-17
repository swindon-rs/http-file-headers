use std::time::SystemTime;

use {AcceptEncoding};


#[derive(Clone, Copy, Debug)]
pub enum Range {
    FromTo(u64, u64),
    AllFrom(u64),
    Last(u64),
}

#[derive(Debug, Clone)]
pub struct Input {
    accept_encoding: AcceptEncoding,
    range: Vec<Range>,
    if_match: Vec<String>,
    if_none: Vec<String>,
    if_unmodified: Option<SystemTime>,
    if_modified: Option<SystemTime>,
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
            accept_encoding: AcceptEncodingParser::new().done(),
            range: Vec::new(),
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
        assert_eq!(size_of::<Input>(), 128);
    }
}
