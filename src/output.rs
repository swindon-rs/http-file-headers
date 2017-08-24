use std::cmp::min;
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::fmt::{self, Display};
use std::fs::{Metadata, File};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

use httpdate::fmt_http_date;

use accept_encoding::Encoding;
use input::{Input};
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

struct LastModified(SystemTime);

pub enum Output {
    NotFound,
    FileHead(Head),
    File(FileWrapper),
    FileRange(FileWrapper),
    Directory,
    InvalidMethod,
    InvalidRange,
}

pub struct Head {
    encoding: Encoding,
    content_length: u64,
    last_modified: Option<LastModified>,
    etag: Etag,
    range: Option<ContentRange>,
}

pub struct ContentRange {
    start: u64,
    end: u64,
    file_size: u64,
}

pub struct FileWrapper {
    head: Head,
    file: File,
    bytes_left: u64,
}

#[derive(Clone, Copy)]
enum HeaderIterState {
    Encoding,
    LastModified,
    Etag,
    ContentRange,
    AcceptRanges,
    Done,
}

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
                H::Encoding => {
                    if self.head.encoding != Encoding::Identity {
                        Some(("Content-Encoding",
                              &self.head.encoding as &Display))
                    } else {
                        None
                    }
                }
                H::LastModified => {
                    self.head.last_modified.as_ref()
                        .map(|x| ("Last-Modified", x as &Display))
                }
                H::Etag => {
                    Some(("Etag", &self.head.etag as &Display))
                }
                H::ContentRange => {
                    self.head.range.as_ref()
                        .map(|x| ("Content-Range", x as &Display))
                }
                H::AcceptRanges => {
                    Some(("Accept-Ranges", BYTES_PTR as &Display))
                }
                H::Done => None,
            };
            self.state = match self.state {
                H::Encoding => H::LastModified,
                H::LastModified => H::Etag,
                H::Etag => H::ContentRange,
                H::ContentRange => H::AcceptRanges,
                H::AcceptRanges => H::Done,
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
    pub fn is_partial(&self) -> bool {
        self.range.is_some()
    }
    pub(crate) fn from_meta(inp: &Input, encoding: Encoding,
        metadata: &Metadata)
        -> Result<Head, Output>
    {
        let mod_time = metadata.modified().ok()
            .and_then(|x| if x < UNIX_EPOCH + Duration::new(MIN_DATE, 0) {
                None
            } else {
                Some(x)
            });
        let size = metadata.len();
        let range = match inp.range {
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
                    end: start + nbytes - 1,
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
            Some(ref rng) => rng.end - rng.start + 1,
            None => size,
        };
        Ok(Head {
            encoding: encoding,
            content_length: clen,
            last_modified: mod_time.map(LastModified),
            etag: Etag::from_metadata(metadata),
            range: range,
        })
    }
    pub fn content_length(&self) -> u64 {
        self.content_length
    }
    pub fn headers(&self) -> HeaderIter {
        HeaderIter {
            head: self,
            state: HeaderIterState::Encoding,
        }
    }
}

impl FileWrapper {
    pub fn new(head: Head, mut file: File) -> Result<FileWrapper, io::Error>
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
    pub fn is_partial(&self) -> bool {
        self.head.range.is_some()
    }
    pub fn content_length(&self) -> u64 {
        self.head.content_length
    }
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

impl fmt::Display for LastModified {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&fmt_http_date(self.0))
    }
}

impl fmt::Display for ContentRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}/{}", self.start, self.end, self.file_size)
    }
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

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert_eq!(size_of::<Output>(), 104);
    }
}
