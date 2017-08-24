use std::u64;
use std::str::from_utf8;


#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slice {
    FromTo(u64, u64),
    AllFrom(u64),
    Last(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Range {
    SingleRangeOfBytes(Slice),
    // TODO(tailhook) support muliple ranges
    //                this requires mutlipart/byteranges though which isn't
    //                easy to implement
    // TODO(tailhook) maybe support other range units
}

pub struct RangeParser {
    // TODO(tailhook) maybe have better error
    result: Result<Option<Range>, ()>,
}


fn parse_slice(slc: &str) -> Result<Slice, ()> {
    let mut pair = slc.splitn(2, "-");
    match (pair.next().map(|x| x.trim()), pair.next().map(|x| x.trim())) {
        (Some(""), Some("")) => Err(()),
        (None, _) => Err(()),
        (_, None) => Err(()),
        (Some(""), Some(x)) => {
            Ok(Slice::Last(x.parse().map_err(|_| ())?))
        }
        (Some(x), Some("")) => {
            Ok(Slice::AllFrom(x.parse().map_err(|_| ())?))
        }
        (Some(x), Some(y)) => {
            let x = x.parse().map_err(|_| ())?;
            let y = y.parse().map_err(|_| ())?;
            if x > y {
                return Err(());
            }
            Ok(Slice::FromTo(x, y))
        }
    }
}

impl Slice {
    fn merge(&mut self, other: Slice) -> bool {
        use self::Slice::*;

        match (self, other) {

            // contained range
            (&mut FromTo(x1, y1), FromTo(x2, y2))
            if x1 >= x2 && y1 <= y2
            => true,

            // reverse contained range
            (&mut FromTo(ref mut x1, ref mut y1), FromTo(x2, y2))
            if x2 >= *x1 && y2 <= *y1
            => {
                *x1 = x2;
                *y1 = y2;
                true
            }

            // adjancent range
            (&mut FromTo(x1, ref mut y1), FromTo(x2, y2))
            if x2 >= x1 && x2 <= *y1+1
            => {
                *y1 = y2;
                true
            }

            // reverse adjacent range
            (&mut FromTo(ref mut x1, _y1), FromTo(x2, y2))
            if y2+1 >= *x1 && x2 < *x1
            => {
                *x1 = x2;
                true
            }

            // TODO(tailhook) cover other cases
            _ => false,
        }
    }
}

fn parse_header(header: &[u8]) -> Result<Range, ()> {
    let header = from_utf8(header).map_err(|_| {
        // Invalid utf-8 in range header
    })?;
    if !header.starts_with("bytes=") {
        // Invalid unit in range header
        return Err(());
    }
    let mut slices = header[6..].split(",");
    let slice = slices.next()
        .ok_or_else(|| {
            // Empty range header
        })?;
    let mut slice = parse_slice(slice)?;
    for item in slices {
        if !slice.merge(parse_slice(item)?) {
            // Can't merge two ranges
            return Err(());
        }
    }
    Ok(Range::SingleRangeOfBytes(slice))
}

impl RangeParser {
    pub fn new() -> RangeParser {
        RangeParser {
            result: Ok(None),
        }
    }
    pub fn add_header(&mut self, header: &[u8]) {
        match self.result {
            Err(()) => {}
            ref mut r @ Ok(Some(_)) => {
                // Duplicate range header
                *r = Err(());
            }
            ref mut r @ Ok(None) => {
                match parse_header(header) {
                    Ok(x) => *r = Ok(Some(x)),
                    Err(()) => *r = Err(()),
                }
            }
        }
    }
    pub fn done(self) -> Result<Option<Range>, ()> {
        self.result
    }
}


#[cfg(test)]
mod test {
    use std::mem::size_of;
    use super::*;

    fn send<T: Send>(_: &T) {}
    fn self_contained<T: 'static>(_: &T) {}

    #[cfg(target_arch="x86_64")]
    #[test]
    fn size() {
        assert_eq!(size_of::<Option<Range>>(), 32);
        assert_eq!(size_of::<Range>(), 24);
        assert_eq!(size_of::<Slice>(), 24);
    }

    #[test]
    fn traits() {
        let v = Range::SingleRangeOfBytes(Slice::FromTo(0, 1));
        send(&v);
        self_contained(&v);
    }

    fn parse(x: &str) -> Result<Option<Range>, ()> {
        let mut parser = RangeParser::new();
        parser.add_header(x.as_bytes());
        parser.done()
    }


    #[test]
    fn parse_range() {
        assert_eq!(parse("bytes=0-1000"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 1000)))));
        assert_eq!(parse("bytes=-1000"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::Last(1000)))));
        assert_eq!(parse("bytes=1000-"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::AllFrom(1000)))));
    }

    #[test]
    fn bad_ranges() {
        assert_eq!(parse("bytes=1000-100"), Err(()));
    }

    #[test]
    fn merge_adjacent() {
        assert_eq!(parse("bytes=0-999, 1000-2000"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 2000)))));
        assert_eq!(parse("bytes=1000-2000, 0-999"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 2000)))));
    }

    #[test]
    fn merge_overlapping() {
        assert_eq!(parse("bytes=0-1000, 1000-2000"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 2000)))));
        assert_eq!(parse("bytes=0-1010, 1000-2000"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 2000)))));
        assert_eq!(parse("bytes=1000-2000, 0-1000"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 2000)))));
        assert_eq!(parse("bytes=1000-2000, 0-1010"),
            Ok(Some(Range::SingleRangeOfBytes(Slice::FromTo(0, 2000)))));
    }

    #[test]
    fn no_merge() {
        assert_eq!(parse("bytes=0-500,1000-2000"), Err(()));
    }

    #[test]
    fn merge_overflow() {
        assert_eq!(parse("bytes=18446744073709551615-18446744073709551615, \
                          18446744073709551615-18446744073709551615"),
            Ok(Some(Range::SingleRangeOfBytes(
                Slice::FromTo(u64::MAX, u64::MAX)))));
    }
}
