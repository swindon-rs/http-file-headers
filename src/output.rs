use std::cmp::min;
use std::fmt::{self, Display};
use std::fs::{Metadata, File};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::time::{UNIX_EPOCH, Duration};
use std::sync::Arc;

use httpdate::HttpDate;

use accept_encoding::Encoding;
use config::Config;
use input::{Input, is_text_file};
use range::{Range, Slice};
use etag::Etag;

/// This is a heuristic that there are no valid dates before 1990-01-01
/// Lower timestamps like 1970-01-01 00:00:01 are used by nixos and some
/// other systems to denote that there is no sensible modification time.
/// Even more zip archives clamp that dates to 1980-01-01.
///
/// All in all we use 1990-01-01 as the minimal date that considered valid,
/// as we don't think anybody serves files with lower date set genuinely.
const MIN_DATE: u64 = 631152000;

const BYTES: &str = "bytes";
const BYTES_PTR: &&str = &BYTES;


#[derive(Debug)]
struct ContentType(&'static str, Arc<Config>);

/// This enum represents all the information needed to form response for
/// the HTTP request
///
/// Variants of this structure represent different modes of responding on
/// request.
#[derive(Debug)]
pub enum Output {
    /// File not found
    NotFound,
    /// File was requested using `HEAD` method
    FileHead(Head),
    /// File is not modified, should return 304
    ///
    /// This might be returned if there is one of `If-None-Match`
    /// or `If-Modified-Since`
    NotModified(Head),
    /// Normal file was requested using `GET` method
    File(FileWrapper),
    /// The `GET` file request includes `Range` field, and range is
    /// contiguous
    FileRange(FileWrapper),
    /// The matching path is a directory
    Directory,
    /// Invalid method was requested
    InvalidMethod,
    /// Invalid `Range` header in request, should return 416
    InvalidRange,
}

/// All the metadata of for the response headers
#[derive(Debug)]
pub struct Head {
    config: Arc<Config>,
    encoding: Encoding,
    content_length: u64,
    content_type: Option<ContentType>,
    last_modified: Option<HttpDate>,
    etag: Option<Etag>,
    range: Option<ContentRange>,
    not_modified: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ContentRange {
    start: u64,
    end: u64,
    file_size: u64,
}

/// Structure that contains all the metadata for response headers and
/// the file which will be sent in response body.
#[derive(Debug)]
pub struct FileWrapper {
    head: Head,
    file: File,
    bytes_left: u64,
}

#[derive(Clone, Copy, Debug)]
enum HeaderIterState {
    LastModified,
    Etag,

    // these not needed if NotModified
    Encoding,
    AcceptRanges,
    ContentRange,
    ContentType,

    Done,
}

#[derive(Debug)]
pub struct HeaderIter<'a> {
    head: &'a Head,
    state: HeaderIterState,
}


impl<'a> Iterator for HeaderIter<'a> {
    type Item=(&'a str, &'a Display);
    fn next(&mut self) -> Option<(&'a str, &'a Display)> {
        use self::HeaderIterState as H;
        loop {
            let value = match self.state {
                H::LastModified => {
                    self.head.last_modified.as_ref()
                        .map(|x| ("Last-Modified", x as &Display))
                }
                H::Etag => {
                    self.head.etag.as_ref()
                        .map(|x| ("ETag", x as &Display))
                }
                H::Encoding => {
                    if self.head.encoding != Encoding::Identity {
                        Some(("Content-Encoding",
                              &self.head.encoding as &Display))
                    } else {
                        None
                    }
                }
                H::ContentRange => {
                    self.head.range.as_ref()
                        .map(|x| ("Content-Range", x as &Display))
                }
                H::ContentType => {
                    self.head.content_type.as_ref()
                        .map(|x| ("Content-Type", x as &Display))
                }
                H::AcceptRanges => {
                    Some(("Accept-Ranges", BYTES_PTR as &Display))
                }
                H::Done => None,
            };
            self.state = match self.state {
                H::LastModified => H::Etag,
                H::Etag if self.head.not_modified => H::Done,
                H::Etag => H::Encoding,
                H::Encoding => H::AcceptRanges,
                H::AcceptRanges => H::ContentRange,
                H::ContentRange => H::ContentType,
                H::ContentType => H::Done,
                H::Done => return None,
            };
            match value {
                Some(x) => return Some(x),
                None => continue,
            }
        }
    }
}

impl Head {
    /// Returns true if response contains partial content (206)
    pub fn is_partial(&self) -> bool {
        self.range.is_some()
    }
    /// Returns true if response is skipped because cache is fresh (304)
    pub fn is_not_modified(&self) -> bool {
        self.not_modified
    }
    pub(crate) fn from_meta(inp: &Input, encoding: Encoding,
        metadata: &Metadata, ctype: &'static str)
        -> Result<Head, Output>
    {
        let mod_time = if inp.config.last_modified {
            metadata.modified().ok()
            .and_then(|x| if x < UNIX_EPOCH + Duration::new(MIN_DATE, 0) {
                None
            } else {
                Some(x)
            })
        } else {
            None
        };
        let size = metadata.len();
        let etag = if inp.config.etag {
            Some(Etag::from_metadata(metadata))
        } else {
            None
        };
        if inp.if_none.len() > 0 {
            if inp.if_none.iter().any(|x| Some(x) == etag.as_ref()) {
                return Err(Output::NotModified(Head {
                    config: inp.config.clone(),
                    encoding: encoding,
                    content_length: 0, // don't need to send
                    content_type: None, // don't need to send
                    last_modified: mod_time.map(Into::into),
                    etag: etag,
                    range: None,
                    not_modified: true,
                }))
            }
        } else if let Some(ref last_mod) = inp.if_modified {
            if mod_time.as_ref().map(|x| last_mod <= x).unwrap_or(false) {
                return Err(Output::NotModified(Head {
                    config: inp.config.clone(),
                    encoding: encoding,
                    content_length: 0, // don't need to send
                    content_type: None, // don't need to send
                    last_modified: mod_time.map(Into::into),
                    etag: etag,
                    range: None,
                    not_modified: true,
                }))
            }
        }
        let (range, clen) = resolve_range(&inp.range, size)?;
        Ok(Head {
            config: inp.config.clone(),
            encoding: encoding,
            content_length: clen,
            content_type: if inp.config.content_type {
                Some(ContentType(ctype, inp.config.clone()))
            } else {
                None
            },
            last_modified: mod_time.map(Into::into),
            etag: etag,
            range: range,
            not_modified: false,
        })
    }
    /// Returns the value of `Content-Length` header that should be sent
    pub fn content_length(&self) -> u64 {
        self.content_length
    }
    /// Returns the iterator over headers to send in response
    ///
    /// Note: this does not include `Content-Length` header,
    /// use `content_length()` method explicitly.
    pub fn headers(&self) -> HeaderIter {
        HeaderIter {
            head: self,
            state: HeaderIterState::LastModified,
        }
    }
}

impl FileWrapper {
    pub(crate) fn new(head: Head, mut file: File)
        -> Result<FileWrapper, io::Error>
    {
        let nbytes = match head.range {
            Some(ContentRange { start, end, .. }) => {
                if start != 0 {
                    file.seek(SeekFrom::Start(start))?;
                }
                end - start + 1
            }
            _ => head.content_length,
        };
        Ok(FileWrapper {
            head: head,
            file: file,
            bytes_left: nbytes,
        })
    }
    /// Returns true if response contains partial content (206)
    pub fn is_partial(&self) -> bool {
        self.head.range.is_some()
    }
    /// Returns the value of `Content-Length` header that should be sent
    pub fn content_length(&self) -> u64 {
        self.head.content_length
    }
    /// Returns the iterator over headers to send in response
    ///
    /// Note: this does not include `Content-Length` header,
    /// use `content_length()` method explicitly.
    pub fn headers(&self) -> HeaderIter {
        self.head.headers()
    }
    /// Read chunk from file into an output file
    ///
    /// **Must be run in disk thread**
    pub fn read_chunk<O>(&mut self, mut output: O) -> io::Result<usize>
        where O: Write
    {
        if self.bytes_left == 0 {
            return Ok(0)
        }
        let mut buf = [0u8; 65536];
        let max = min(buf.len() as u64, self.bytes_left) as usize;
        let bytes = self.file.read(&mut buf[..max])?;
        let wbytes = match output.write(&buf[..bytes]) {
            Ok(wbytes) if wbytes != bytes => {
                assert!(wbytes < bytes);
                self.file.seek(SeekFrom::Current(
                    - ((bytes - wbytes) as i64)))?;
                wbytes
            }
            Ok(wbytes) => wbytes,
            Err(e) => {
                // Probaby it's WouldBlock, but let's rewind on anything
                self.file.seek(SeekFrom::Current(- (bytes as i64)))?;
                return Err(e);
            }
        };
        self.bytes_left -= wbytes as u64;
        Ok(wbytes)
    }
}

impl Output {
}

impl fmt::Display for ContentRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.file_size == 0 {
            write!(f, "bytes */0")
        } else {
            write!(f, "bytes {}-{}/{}", self.start, self.end, self.file_size)
        }
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if is_text_file(self.0) {
            if let Some(ref charset) = self.1.text_charset {
                write!(f, "{}; charset={}", self.0, charset)
            } else {
                f.write_str(self.0)
            }
        } else {
            f.write_str(self.0)
        }
    }
}

fn resolve_range(inp_range: &Option<Range>, size: u64)
    -> Result<(Option<ContentRange>, u64), Output>
{
    let range = match *inp_range {
        Some(Range::SingleRangeOfBytes(Slice::FromTo(s, e))) => {
            if s >= size {
                return Err(Output::InvalidRange);
            } else {
                let nbytes = min(size - s, (e - s).saturating_add(1));
                Some(ContentRange {
                    start: s,
                    end: s + nbytes - 1,
                    file_size: size,
                })
            }
        }
        Some(Range::SingleRangeOfBytes(Slice::Last(mut nbytes))) => {
            let start = if nbytes > size {
                nbytes = size;
                0
            } else {
                size - nbytes
            };
            Some(ContentRange {
                start: start,
                end: (start + nbytes).saturating_sub(1),
                file_size: size,
            })
        }
        Some(Range::SingleRangeOfBytes(Slice::AllFrom(start))) => {
            if start >= size {
                return Err(Output::InvalidRange);
            } else {
                Some(ContentRange {
                    start: start,
                    end: size - 1,
                    file_size: size,
                })
            }
        }
        None => None,
    };
    let clen = match range {
        Some(_) if size == 0 => 0,
        Some(ref rng) => rng.end - rng.start + 1,
        None => size,
    };
    return Ok((range, clen));
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use super::*;

    fn send<T: Send>(_: &T) {}
    fn self_contained<T: 'static>(_: &T) {}

    #[test]
    #[cfg(unix)]
    fn traits() {
        let v = Output::NotFound;
        send(&v);
        self_contained(&v);
    }

    #[cfg(all(target_arch="x86_64", target_os="linux"))]
    #[test]
    fn size() {
        assert_eq!(size_of::<Output>(), 128);
    }

    #[test]
    fn format_range() {
        assert_eq!(format!("{}", ContentRange {
            start: 10,
            end: 100,
            file_size: 1000,
        }), "bytes 10-100/1000");
    }

    #[test]
    fn format_zero_file_size() {
        assert_eq!(format!("{}", ContentRange {
            start: 0,
            end: 0,
            file_size: 0,
        }), "bytes */0");
    }

    fn last(num: u64) -> Range {
        Range::SingleRangeOfBytes(Slice::Last(num))
    }

    fn from(num: u64) -> Range {
        Range::SingleRangeOfBytes(Slice::AllFrom(num))
    }

    fn range(from: u64, to: u64) -> Range {
        Range::SingleRangeOfBytes(Slice::FromTo(from, to))
    }

    fn res(start: u64, end: u64, size: u64) -> ContentRange {
        ContentRange {
            start: start,
            end: end,
            file_size: size,
        }
    }
    fn resolve(rng: Range, file_size: u64) -> ContentRange {
        resolve_range(&Some(rng), file_size).unwrap().0.unwrap()
    }
    fn resolve_clen(rng: Range, file_size: u64) -> u64 {
        resolve_range(&Some(rng), file_size).unwrap().1
    }

    #[test]
    fn range_on_zero_length() {
        assert_eq!(resolve(last(100), 0), res(0, 0, 0));
        assert_eq!(resolve_clen(last(100), 0), 0);
        resolve_range(&Some(from(100)), 0).unwrap_err();
        resolve_range(&Some(range(0, 100)), 0).unwrap_err();
    }

    #[test]
    fn range_on_short() {
        assert_eq!(resolve(last(1000), 100), res(0, 99, 100));
        assert_eq!(resolve_clen(last(1000), 100), 100);
        resolve_range(&Some(range(1000, 2000)), 100).unwrap_err();
        assert_eq!(resolve(range(10, 1000), 100), res(10, 99, 100));
        assert_eq!(resolve_clen(range(10, 1000), 100), 90);
    }

    #[test]
    fn norm_ranges() {
        assert_eq!(resolve(last(1000), 10000), res(9000, 9999, 10000));
        assert_eq!(resolve(range(100, 1000), 10000), res(100, 1000, 10000));
        assert_eq!(resolve(from(777), 10000), res(777, 9999, 10000));
    }
}
