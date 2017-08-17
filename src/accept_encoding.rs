use std::collections::HashMap;
use std::str::from_utf8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Encoding {
    Identity,
    Gzip,
    Brotli,
    #[doc(hidden)]
    __Nonexhaustive,
}

#[derive(Debug, Clone)]
pub struct AcceptEncoding {
    ordered: [Encoding; 3],
}

/// Parser for accept encoding header
///
/// It drops unaccepted encodings and returns only supported ones
pub struct AcceptEncodingParser {
    buf: Vec<(Encoding, u16 /*0..1000*/)>,
    allow_identity: bool,
    allow_any: bool,
}

fn parse_q(val: Option<&[u8]>) -> Option<u16> {
    if let Some(qbytes) = val {
        if let Ok(qstr) = from_utf8(qbytes) {
            let qstr = qstr.trim();
            if qstr.starts_with("q=") && qstr.len() <= 7 {
                if qstr.as_bytes()[2] == b'1' {
                    if qstr.len() == 3 || qstr.as_bytes()[3] == b'.' &&
                        qstr.as_bytes()[4..].iter().all(|&x| x == b'0')
                    {
                        return Some(1000);
                    } else {
                        return None;
                    }
                } else if qstr.as_bytes()[2] == b'0' {
                    if qstr.len() == 3 {
                        return Some(0)
                    } else if qstr.as_bytes()[3] != b'.' {
                        return None;
                    } else {
                        let mut val = 0;
                        for i in 0..qstr.len()-4 {
                            match qstr.as_bytes()[i+4] {
                                x@b'0'...b'9' => {
                                    val += (x - b'0') as u16 * 10u16.pow((2-i) as u32);
                                }
                                _ => return None,
                            }
                        }
                        return Some(val);
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        } else {
            return None;
        }
    } else {
        return Some(1000)
    }
}

impl AcceptEncodingParser {
    pub fn new() -> AcceptEncodingParser {
        AcceptEncodingParser {
            buf: Vec::new(),
            allow_identity: true,
            allow_any: true,
        }
    }
    fn add_chunk(&mut self, chunk: &[u8]) {
        use self::Encoding::*;
        let mut piter = chunk.split(|&x| x == b';');
        let enc = piter.next().and_then(|x| from_utf8(x).ok()).map(str::trim);
        let enc = match enc {
            Some("identity") => Some(Identity),
            Some("br") => Some(Brotli),
            Some("gzip") => Some(Gzip),
            Some("*") => None,
            _ => return,
        };
        let q = if let Some(q) = parse_q(piter.next()) {
            q
        } else {
            return;
        };
        match (enc, q) {
            (None, 0) => self.allow_any = false,
            (None, _) => {}, // useless?
            (Some(x), _) => self.buf.push((x, q)),
        }
    }
    pub fn add_header(&mut self, header: &[u8]) {
        for chunk in header.split(|&x| x == b',') {
            self.add_chunk(chunk)
        }
    }
    pub fn done(&mut self) -> AcceptEncoding {
        unimplemented!();
    }
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use super::*;

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert_eq!(size_of::<Option<Encoding>>(), 2);
    }

    #[test]
    fn parse_q_none() {
        assert_eq!(parse_q(None), Some(1000));
    }

    #[test]
    fn parse_q_one() {
        assert_eq!(parse_q(Some(b"q=1")), Some(1000));
        assert_eq!(parse_q(Some(b"q=1.0")), Some(1000));
        assert_eq!(parse_q(Some(b"q=1.00")), Some(1000));
        assert_eq!(parse_q(Some(b"q=1.000")), Some(1000));
    }

    #[test]
    fn parse_q_bad() {
        assert_eq!(parse_q(Some(b"q=1.1")), None);
        assert_eq!(parse_q(Some(b"q=0.0000")), None);
        assert_eq!(parse_q(Some(b"q=1.0000")), None);
        assert_eq!(parse_q(Some(b"q=1.37372")), None);
        assert_eq!(parse_q(Some(b"q=0.37372")), None);
        assert_eq!(parse_q(Some(b"q=2.0")), None);
    }

    #[test]
    fn parse_q_norm() {
        assert_eq!(parse_q(Some(b"q=0")), Some(0));
        assert_eq!(parse_q(Some(b"q=0.0")), Some(0));
        assert_eq!(parse_q(Some(b"q=0.00")), Some(0));
        assert_eq!(parse_q(Some(b"q=0.000")), Some(0));
        assert_eq!(parse_q(Some(b"q=0")), Some(0));
        assert_eq!(parse_q(Some(b"q=0.1")), Some(100));
        assert_eq!(parse_q(Some(b"q=0.23")), Some(230));
        assert_eq!(parse_q(Some(b"q=0.456")), Some(456));
    }
}
