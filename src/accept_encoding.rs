use std::fmt;
use std::str::from_utf8;
use std::slice;

/// Single encoding that might be accepted by user agent
///
/// Note: We only support fixed set of encodings, the most useful ones. We
/// have no plans on adding open-ended encodings because it doesn't make
/// much sense, still we may add some encoding in future, based on it's
/// popularity and browser support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Encoding {
    /// Brotli encoding (trasferred as "br", and has same extension)
    Brotli,
    /// Gzip encoding (trasferred as "gzip", and extension ".gz")
    Gzip,
    /// Identity means no encoding
    Identity,
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
    /// TODO(tailhook) it's unclear what to do with `allow_any`
    allow_any: bool,
}

/// Iterator over encodings in preferred order
///
/// You may create one using `Input::encodings()`
#[derive(Debug)]
pub struct Iter<'a> {
    slice: slice::Iter<'a, Encoding>,
    identity: bool,
}

impl Encoding {
    /// Returns default filename suffix used for this encoding when reading
    /// a file from a filesystem.
    pub fn suffix(&self) -> &'static str {
        use self::Encoding::*;
        match *self {
            Identity => "",
            Gzip => ".gz",
            Brotli => ".br",
            __Nonexhaustive => unimplemented!(),
        }
    }
}

impl AcceptEncoding {
    pub fn iter(&self) -> Iter {
        Iter {
            slice: self.ordered.iter(),
            identity: false,
        }
    }
    pub fn identity() -> AcceptEncoding {
        AcceptEncoding {
            ordered: [Encoding::Identity; 3],
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Encoding;
    fn next(&mut self) -> Option<Encoding> {
        loop {
            match self.slice.next() {
                Some(&Encoding::Identity) if !self.identity => {
                    self.identity = true;
                    break Some(Encoding::Identity)
                }
                Some(&Encoding::Identity) => {}
                Some(&Encoding::__Nonexhaustive) => unreachable!(),
                Some(value) => break Some(*value),
                None => break None,
            }
        }
    }
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
    pub fn done(mut self) -> AcceptEncoding {
        self.buf.sort_by(|&(a, qa), &(b, qb)|
            qb.cmp(&qa).then(a.cmp(&b)));
        let mut result = AcceptEncoding {
            ordered: [Encoding::Identity; 3],
        };
        // TODO(tailhook) process disabled (q=0) encodings
        let it = self.buf.iter().filter(|&&(_, q)| q != 0).take(3).enumerate();
        for (i, &(e, _)) in it {
            result.ordered[i] = e;
        }
        return result;
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Encoding::*;
        match *self {
            Brotli => f.write_str("br"),
            Gzip => f.write_str("gzip"),
            Identity => f.write_str("identity"),
            __Nonexhaustive => unreachable!(),
        }
    }
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use super::*;

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert!(size_of::<Option<Encoding>>() <= 2);
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

    fn to_ext(h: &str) -> Vec<&'static str> {
        let mut parser = AcceptEncodingParser::new();
        parser.add_header(h.as_bytes());
        let ae = parser.done();
        ae.iter().map(|x| x.suffix()).collect()
    }

    #[test]
    fn test_norm() {
        assert_eq!(to_ext(""), vec![""]);
    }

    #[test]
    fn test_br() {
        assert_eq!(to_ext("br"), vec![".br", ""]);
    }

    #[test]
    fn test_gz() {
        assert_eq!(to_ext("gzip"), vec![".gz", ""]);
    }

    #[test]
    fn test_br_gz() {
        assert_eq!(to_ext("br, gzip"), vec![".br", ".gz", ""]);
    }

    #[test]
    fn test_gz_br() {
        // same weight, brotli wins, as it compresses better
        assert_eq!(to_ext("gzip, br"), vec![".br", ".gz", ""]);
    }

    #[test]
    fn test_gz_br_q() {
        assert_eq!(to_ext("gzip, br;q=0.5"), vec![".gz", ".br", ""]);
    }
    #[test]
    fn test_identity() {
        assert_eq!(to_ext("identity"), vec![""]);
        assert_eq!(to_ext("gzip, br, identity"), vec![".br", ".gz", ""]);
        assert_eq!(to_ext("identity, br"), vec![".br", ""]);
        assert_eq!(to_ext("identity, br;q=0.5"), vec!["", ".br"]);
    }
}
