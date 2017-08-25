use std::str::from_utf8;
use std::time::SystemTime;

use httpdate;
use etag::Etag;


pub struct ModifiedParser {
    result: Result<Option<SystemTime>, ()>,
}

pub struct NoneMatchParser {
    etags: Vec<Etag>,
}


impl ModifiedParser {
    pub fn new() -> ModifiedParser {
        ModifiedParser {
            result: Ok(None),
        }
    }
    pub fn add_header(&mut self, header: &[u8]) {
        match self.result {
            Err(()) => {}
            ref mut r @ Ok(Some(_)) => {
                // Duplicate if_modified_since header
                *r = Err(());
            }
            ref mut r @ Ok(None) => {
                let res = from_utf8(header).ok()
                    .and_then(|s| httpdate::parse_http_date(s).ok());
                match res {
                    Some(x) => *r = Ok(Some(x)),
                    None => *r = Err(()),
                }
            }
        }
    }
    pub fn done(self) -> Option<SystemTime> {
        self.result
            // Treating invalid or duplicate header as no header at all
            .unwrap_or_else(|()| None)
    }
}

impl NoneMatchParser {
    pub fn new() -> NoneMatchParser {
        NoneMatchParser {
            etags: Vec::new(),
        }
    }
    fn add_chunk(&mut self, mut chunk: &[u8]) {
        while chunk.len() > 0 && chunk[0] == b' ' {
            chunk = &chunk[1..];
        }
        if chunk.len() < 4 + 16 {  // the 'W/"xx"' and 16 bytes of base64
            // Is not our etag
            return;
        }
        if chunk[0] != b'W' || chunk[1] != b'/' || chunk[2] != b'"' ||
            chunk[16+3] != b'"'
        {
            // Is not a weak tag (or wrong length)
            return;
        }
        if !chunk[16+4..].iter().all(|&x| x == b' ') {
            // invalid trailing bytes
            return;
        }
        match Etag::decode_base64(&chunk[3..16+3]) {
            Ok(etag) => self.etags.push(etag),
            Err(()) => return, // skip invalid tags
        }
    }
    pub fn add_header(&mut self, header: &[u8]) {
        for chunk in header.split(|&x| x == b',') {
            self.add_chunk(chunk);
        }
    }
    pub fn done(self) -> Vec<Etag> {
        self.etags
    }
}

#[cfg(test)]
mod test {
    use std::time::{SystemTime, Duration, UNIX_EPOCH};
    use etag::Etag;
    use super::*;

    fn parse_etag(val: &str) -> Vec<Etag> {
        let mut parser = NoneMatchParser::new();
        parser.add_header(val.as_bytes());
        parser.done()
    }

    fn parse_mod(val: &str) -> Option<SystemTime> {
        let mut parser = ModifiedParser::new();
        parser.add_header(val.as_bytes());
        parser.done()
    }

    #[test]
    fn single_etag() {
        assert_eq!(parse_etag(r#"W/"tYJT9KJUI0KX2I5q""#), vec![
            Etag([181, 130, 83, 244, 162, 84, 35, 66, 151, 216, 142, 106])
        ]);
        assert_eq!(parse_etag(r#"    W/"tYJT9KJUI0KX2I5q"  "#), vec![
            Etag([181, 130, 83, 244, 162, 84, 35, 66, 151, 216, 142, 106])
        ]);
    }

    #[test]
    fn two_tags() {
        assert_eq!(parse_etag(r#"W/"tYJT9KJUI0KX2I5q", W/"tYJT9KJUI0KX2I5q""#),
        vec![
            Etag([181, 130, 83, 244, 162, 84, 35, 66, 151, 216, 142, 106]),
            Etag([181, 130, 83, 244, 162, 84, 35, 66, 151, 216, 142, 106]),
        ]);
    }

    #[test]
    fn last_modified() {
        assert_eq!(parse_mod(r#"Tue, 22 Aug 2017 20:47:13 GMT"#),
            Some(UNIX_EPOCH + Duration::new(1503434833, 0)));
    }

    #[test]
    fn bad_etags() {
        assert_eq!(parse_etag(r#"W/"tYJT9KJ^^UI0KX2I5q""#), vec![]);
        assert_eq!(parse_etag(r#""tYJT9KJUI0KX2I5q""#), vec![]);
        assert_eq!(parse_etag(r#""tYJT9KJUI  0KX2I5q""#), vec![]);
        assert_eq!(parse_etag(r#""tYJT9KJUI0KX2I5q"+1"#), vec![]);
        assert_eq!(parse_etag(r#"X/"tYJT9KJUI0KX2I5q""#), vec![]);
    }
}
