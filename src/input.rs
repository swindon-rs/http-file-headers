use std::io;
use std::time::SystemTime;
use std::ascii::AsciiExt;
use std::fs::{File};
use std::path::Path;
use std::ffi::OsString;
use std::sync::Arc;

use accept_encoding::{AcceptEncodingParser, Iter as EncodingIter};
use config::Config;
use conditionals::{ModifiedParser, NoneMatchParser};
use etag::Etag;
use output::{Head, FileWrapper};
use range::{Range, RangeParser};
use {AcceptEncoding, Output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Head,
    Get,
    InvalidMethod,
    InvalidRange,
}

#[derive(Debug, Clone)]
pub struct Input {
    pub(crate) config: Arc<Config>,
    pub(crate) mode: Mode,
    pub(crate) accept_encoding: AcceptEncoding,
    pub(crate) range: Option<Range>,
    pub(crate) if_range: Option<Result<SystemTime, Etag>>,
    pub(crate) if_match: Vec<Etag>,
    pub(crate) if_none: Vec<Etag>,
    pub(crate) if_unmodified: Option<SystemTime>,
    pub(crate) if_modified: Option<SystemTime>,
}

impl Input {
    pub fn from_headers<'x, I>(cfg: &Arc<Config>, method: &str, headers: I)
        -> Input
        where I: Iterator<Item=(&'x str, &'x[u8])>
    {
        let mode = match method {
            "HEAD" => Mode::Head,
            "GET" => Mode::Get,
            _ => return Input {
                config: cfg.clone(),
                mode: Mode::InvalidMethod,
                accept_encoding: AcceptEncoding::identity(),
                range: None,
                if_range: None,
                if_match: Vec::new(),
                if_none: Vec::new(),
                if_unmodified: None,
                if_modified: None,
            },
        };
        let mut ae_parser = AcceptEncodingParser::new();
        let mut range_parser = RangeParser::new();
        let mut modified_parser = ModifiedParser::new();
        let mut none_match_parser = NoneMatchParser::new();
        for (key, val) in headers {
            if key.eq_ignore_ascii_case("accept-encoding") {
                ae_parser.add_header(val);
            } else if key.eq_ignore_ascii_case("range") {
                range_parser.add_header(val);
            } else if key.eq_ignore_ascii_case("if-modified-since") {
                modified_parser.add_header(val);
            } else if key.eq_ignore_ascii_case("if-none-match") {
                none_match_parser.add_header(val);
            }
        }
        let range = match range_parser.done() {
            Ok(range) => range,
            Err(()) => return Input {
                config: cfg.clone(),
                mode: Mode::InvalidRange,
                accept_encoding: AcceptEncoding::identity(),
                range: None,
                if_range: None,
                if_match: Vec::new(),
                if_none: Vec::new(),
                if_unmodified: None,
                if_modified: None,
            },
        };
        Input {
            config: cfg.clone(),
            mode: mode,
            accept_encoding: ae_parser.done(),
            range: range,
            if_range: None,
            if_match: Vec::new(),
            if_none: none_match_parser.done(),
            if_unmodified: None,
            if_modified: modified_parser.done(),
        }
    }
    pub fn encodings(&self) -> EncodingIter {
        self.accept_encoding.iter()
    }
    /// Open files from filesystem
    ///
    /// **Must be run in disk thread**
    pub fn probe_file<P: AsRef<Path>>(&self, base_path: P)
        -> Result<Output, io::Error>
    {
        match self.mode {
            Mode::Head | Mode::Get => {}
            Mode::InvalidMethod => return Ok(Output::InvalidMethod),
            Mode::InvalidRange => return Ok(Output::InvalidRange),
        }
        let path = base_path.as_ref().as_os_str();
        let mut buf = OsString::with_capacity(path.len() + 3);
        for enc in self.encodings() {
            buf.clear();
            buf.push(path);
            buf.push(enc.suffix());
            let path = Path::new(&buf);
            match File::open(path).and_then(|f| f.metadata().map(|m| (f, m))) {
                Ok((f, meta)) => {
                    if meta.is_dir() {
                        return Ok(Output::Directory);
                    }
                    let head = match Head::from_meta(self, enc, &meta,
                                                     base_path.as_ref())
                    {
                        Err(output) => return Ok(output),
                        Ok(head) => head,
                    };
                    match self.mode {
                        Mode::InvalidMethod => unreachable!(),
                        Mode::InvalidRange => unreachable!(),
                        Mode::Head => return Ok(Output::FileHead(head)),
                        Mode::Get => {
                            return Ok(Output::File(
                                FileWrapper::new(head, f)?));
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        return Ok(Output::NotFound);
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
            config: Config::new().done(),
            mode: Mode::Get,
            accept_encoding: AcceptEncodingParser::new().done(),
            range: None,
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
        assert_eq!(size_of::<Input>(), 176);
    }
}
