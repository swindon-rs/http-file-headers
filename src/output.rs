use std::io::{self, Read, Write};
use std::fmt::{self, Display};
use std::fs::{Metadata, File};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

use httpdate::fmt_http_date;

use accept_encoding::Encoding;
use input::{Mode, Input};
use etag::Etag;

/// This is a heuristic that there are no valid dates before 1990-01-01
/// Lower timestamps like 1970-01-01 00:00:01 are used by nixos and some
/// other systems to denote that there is no sensible modification time.
/// Even more zip archives clamp that dates to 1980-01-01.
///
/// All in all we use 1990-01-01 as the minimal date that considered valid,
/// as we don't think anybody serves files with lower date set genuinely.
const MIN_DATE: u64 = 631152000;

struct LastModified(SystemTime);

pub struct Output {
    mode: Mode,
    encoding: Encoding,
    content_length: u64,
    last_modified: Option<LastModified>,
    etag: Etag,
    file: File,
}

#[derive(Clone, Copy)]
enum HeaderIterState {
    Encoding,
    LastModified,
    Etag,
    Done,
}

pub struct HeaderIter<'a> {
    out: &'a Output,
    state: HeaderIterState,
}


impl<'a> Iterator for HeaderIter<'a> {
    type Item=(&'a str, &'a Display);
    fn next(&mut self) -> Option<(&'a str, &'a Display)> {
        use self::HeaderIterState as H;
        loop {
            let value = match self.state {
                H::Encoding => {
                    if self.out.encoding != Encoding::Identity {
                        Some(("Content-Encoding",
                              &self.out.encoding as &Display))
                    } else {
                        None
                    }
                }
                H::LastModified => {
                    self.out.last_modified.as_ref()
                        .and_then(|x| Some(("Last-Modified", x as &Display)))
                }
                H::Etag => {
                    Some(("Etag", &self.out.etag as &Display))
                }
                H::Done => None,
            };
            self.state = match self.state {
                H::Encoding => H::LastModified,
                H::LastModified => H::Etag,
                H::Etag => H::Done,
                H::Done => return None,
            };
            match value {
                Some(x) => return Some(x),
                None => continue,
            }
        }
    }
}

impl Output {
    pub fn from_file(inp: &Input, encoding: Encoding,
        metadata: &Metadata, file: File)
        -> Output
    {
        let mod_time = metadata.modified().ok()
            .and_then(|x| if x < UNIX_EPOCH + Duration::new(MIN_DATE, 0) {
                None
            } else {
                Some(x)
            });
        Output {
            mode: inp.mode,
            encoding: encoding,
            content_length: metadata.len(),
            last_modified: mod_time.map(LastModified),
            etag: Etag::from_metadata(metadata),
            file: file,
        }
    }
    pub fn content_length(&self) -> u64 {
        self.content_length
    }
    pub fn headers(&self) -> HeaderIter {
        HeaderIter {
            out: self,
            state: HeaderIterState::Encoding,
        }
    }
    /// Read chunk from file into an output file
    ///
    /// **Must be run in disk thread**
    pub fn read_chunk<O>(&mut self, mut output: O) -> io::Result<usize>
        where O: Write
    {
        let mut buf = [0u8; 65536];
        let bytes = self.file.read(&mut buf)?;
        // TODO(tailhook) rewind or poison this file on error
        let wbytes = output.write(&buf[..bytes])?;
        // TODO(tailhook) rewind file on less bytes
        assert_eq!(wbytes, bytes);
        Ok(wbytes)
    }
}

impl fmt::Display for LastModified {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&fmt_http_date(self.0))
    }
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use accept_encoding::{Encoding};
    use super::*;

    fn send<T: Send>(_: &T) {}
    fn self_contained<T: 'static>(_: &T) {}

    #[test]
    #[cfg(unix)]
    fn traits() {
        let f = File::open("/dev/null").unwrap();
        let v = Output {
            mode: Mode::Get,
            encoding: Encoding::Identity,
            content_length: 192,
            last_modified: None,
            etag: Etag::from_metadata(&f.metadata().unwrap()),
            file: f,
        };
        send(&v);
        self_contained(&v);
    }

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert_eq!(size_of::<Output>(), 56);
    }
}
