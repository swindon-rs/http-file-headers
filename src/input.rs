use std::io;
use std::time::SystemTime;
use std::ascii::AsciiExt;
use std::fs::{File};
use std::path::Path;
use std::ffi::OsString;
use std::sync::Arc;

use accept_encoding::{AcceptEncoding, AcceptEncodingParser};
use accept_encoding::{Iter as EncodingIter, Encoding};
use config::{Config, EncodingSupport};
use conditionals::{ModifiedParser, NoneMatchParser};
use etag::Etag;
use output::{Head, FileWrapper};
use range::{Range, RangeParser};
use mime_guess::get_mime_type_str;
use {Output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Head,
    Get,
    InvalidMethod,
    InvalidRange,
}

pub fn is_text_file(val: &str) -> bool {
    return val.starts_with("text/") || val == "application/javascript"
}

/// The structure represents parsed input headers
///
/// Create it with `Input::from_headers`, and make output structure
/// using `Input::probe_file`. Note: the latter should be run in disk
/// thread.
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
    /// A constructor for `Input` object
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
            if cfg.encoding_support != EncodingSupport::Never &&
               key.eq_ignore_ascii_case("accept-encoding")
            {
                ae_parser.add_header(val);
            } else if key.eq_ignore_ascii_case("range") {
                range_parser.add_header(val);
            } else if cfg.last_modified &&
                      key.eq_ignore_ascii_case("if-modified-since")
            {
                modified_parser.add_header(val);
            } else if cfg.etag &&
                      key.eq_ignore_ascii_case("if-none-match")
            {
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
    /// Iterate over encodings accepted by user-agent in preferred order
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
        let base_path = base_path.as_ref();
        match base_path.metadata() {
            Ok(ref m) if m.is_dir() => self.try_dir(base_path),
            Ok(_) => self.try_file(base_path),
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Output::NotFound);
            }
            Err(e) => return Err(e),
        }
    }
    fn try_dir(&self, base_path: &Path) -> Result<Output, io::Error> {
        let mut buf = base_path.to_path_buf();
        for name in &self.config.index_files {
            buf.push(name);
            if buf.exists() {
                return self.try_file(&buf);
            }
            buf.pop();
        }
        Ok(Output::Directory)
    }
    fn try_file(&self, base_path: &Path) -> Result<Output, io::Error> {
        use config::EncodingSupport as E;
        let ctype = base_path.extension()
            .and_then(|x| x.to_str())
            .and_then(|x| get_mime_type_str(x))
            .unwrap_or("application/octed-stream");
        let encodings = match self.config.encoding_support {
            E::Never => false,
            E::TextFiles => is_text_file(ctype),
            E::AllFiles => true,
        };
        if encodings {
            return self.try_encodings(base_path, ctype);
        } else {
            return self.try_path(base_path, Encoding::Identity, ctype);
        }
    }

    fn try_path(&self, path: &Path, enc: Encoding, ctype: &'static str)
        -> Result<Output, io::Error>
    {
        let f = File::open(path)?;
        let meta = f.metadata()?;
        if meta.is_dir() {
            return Err(io::ErrorKind::NotFound.into());
        }
        let head = match Head::from_meta(self, enc, &meta, ctype) {
            Err(output) => return Ok(output),
            Ok(head) => head,
        };
        match self.mode {
            Mode::InvalidMethod => unreachable!(),
            Mode::InvalidRange => unreachable!(),
            Mode::Head => Ok(Output::FileHead(head)),
            Mode::Get => Ok(Output::File(FileWrapper::new(head, f)?)),
        }
    }

    fn try_encodings(&self, base_path: &Path, ctype: &'static str)
        -> Result<Output, io::Error>
    {
        let path = base_path.as_os_str();
        let mut buf = OsString::with_capacity(path.len() + 3);
        for enc in self.encodings() {
            buf.clear();
            buf.push(path);
            buf.push(enc.suffix());
            let path = Path::new(&buf);
            match self.try_path(&path, enc, ctype) {
                Ok(x) => return Ok(x),
                Err(ref e) if e.kind() == io::ErrorKind::NotFound
                => continue,
                Err(e) => return Err(e),
            }
        }
        // Tecnically it can happen only if file was removed while
        // we are looking for encodings
        Ok(Output::NotFound)
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
