use std::fmt::Display;
use std::fs::Metadata;

use accept_encoding::Encoding;
use input::{Mode, Input};


pub struct Output {
    mode: Mode,
    encoding: Encoding,
    content_length: u64,
}

#[derive(Clone, Copy)]
enum HeaderIterState {
    Encoding,
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
                H::Done => None,
            };
            self.state = match self.state {
                H::Encoding => H::Done,
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
    pub fn from_file(inp: &Input, encoding: Encoding, metadata: &Metadata)
        -> Output
    {
        Output {
            mode: inp.mode,
            encoding: encoding,
            content_length: metadata.len(),
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
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use accept_encoding::{Encoding};
    use super::*;

    fn send<T: Send>(_: &T) {}
    fn self_contained<T: 'static>(_: &T) {}

    #[test]
    fn traits() {
        let v = Output {
            mode: Mode::Get,
            encoding: Encoding::Identity,
            content_length: 192
        };
        send(&v);
        self_contained(&v);
    }

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert_eq!(size_of::<Output>(), 16);
    }
}
