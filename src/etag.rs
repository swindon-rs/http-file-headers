use std::io::Write;
use std::fs::Metadata;
use std::fmt;
use std::time::{Duration, UNIX_EPOCH};
use std::str::from_utf8_unchecked;

use blake2::{Blake2b, Digest};
use digest_writer::Writer;
use typenum::U12;
use byteorder::{WriteBytesExt, BigEndian};


#[derive(Clone, PartialEq, Eq)]
pub struct Etag(pub(crate) [u8; 12]);


impl Etag {
    pub fn from_metadata(metadata: &Metadata) -> Etag {
        let mut wr = Writer::new(Blake2b::<U12>::new());
        wr.write_u64::<BigEndian>(metadata.len()).unwrap();
        let fmod = metadata.modified().ok()
            .and_then(|x| x.duration_since(UNIX_EPOCH).ok())
            .unwrap_or(Duration::new(0, 0));
        wr.write_u64::<BigEndian>(fmod.as_secs()).unwrap();
        wr.write_u32::<BigEndian>(fmod.subsec_nanos()).unwrap();
        let fcreated = metadata.created().ok()
            .and_then(|x| x.duration_since(UNIX_EPOCH).ok())
            .unwrap_or(Duration::new(0, 0));
        wr.write_u64::<BigEndian>(fcreated.as_secs()).unwrap();
        wr.write_u32::<BigEndian>(fcreated.subsec_nanos()).unwrap();
        extra(&mut wr, metadata);
        let digest = wr.into_inner();
        let mut value = [0u8; 12];
        value.copy_from_slice(&digest.result()[..]);
        return Etag(value);
    }
    pub(crate) fn decode_base64(slice: &[u8]) -> Result<Etag, ()> {
        debug_assert!(slice.len() == 16);
        let mut value = [0u8; 12];
        decode4(&slice[..4], &mut value[..3])?;
        decode4(&slice[4..8], &mut value[3..6])?;
        decode4(&slice[8..12], &mut value[6..9])?;
        decode4(&slice[12..], &mut value[9..])?;
        Ok(Etag(value))
    }
}

#[cfg(unix)]
fn extra<W: Write>(wr: &mut W, metadata: &Metadata) {
    use std::os::unix::fs::MetadataExt;
    // sometimes last_modified date is not reliable
    // so we use inode number and `ctime` date on unix systems too
    wr.write_u64::<BigEndian>(metadata.dev()).unwrap();
    wr.write_u64::<BigEndian>(metadata.ino()).unwrap();
    wr.write_i64::<BigEndian>(metadata.ctime()).unwrap();
    wr.write_i64::<BigEndian>(metadata.ctime_nsec()).unwrap();
}

#[cfg(not(unix))]
fn extra<W: Write>(_: &mut W, _: &metadata) {
}

#[inline(always)]
fn base64triple(src: &[u8], dest: &mut [u8]) {
    // url-safe base64 chars
    const CHARS: &'static[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                  abcdefghijklmnopqrstuvwxyz\
                                  0123456789-_";
    debug_assert!(src.len() == 3);
    debug_assert!(dest.len() == 4);
    let n = ((src[0] as usize) << 16) |
            ((src[1] as usize) <<  8) |
             (src[2] as usize) ;
    dest[0] = CHARS[(n >> 18) & 63];
    dest[1] = CHARS[(n >> 12) & 63];
    dest[2] = CHARS[(n >>  6) & 63];
    dest[3] = CHARS[(n >>  0) & 63];
}

#[inline(always)]
fn decode_char(c: u8) -> Result<u8, ()> {
    match c {
        b'-' => Ok(62),
        b'_' => Ok(63),
        b'0'...b'9' => Ok(c - b'0' + 52),
        b'A'...b'Z' => Ok(c - b'A'),
        b'a'...b'z' => Ok(c - b'a' + 26),
        _ => Err(()),
    }
}

#[inline(always)]
fn decode4(src: &[u8], dest: &mut [u8]) -> Result<(), ()> {
    debug_assert!(src.len() == 4);
    debug_assert!(dest.len() == 3);
    let c1 = decode_char(src[0])?;
    let c2 = decode_char(src[1])?;
    let c3 = decode_char(src[2])?;
    let c4 = decode_char(src[3])?;
    let n = ((c1 as u32) << 18) |
            ((c2 as u32) << 12) |
            ((c3 as u32) <<  6) |
            (c4 as u32);
    dest[0] = ((n >> 16) & 0xFF) as u8;
    dest[1] = ((n >>  8) & 0xFF) as u8;
    dest[2] = ((n >>  0) & 0xFF) as u8;
    Ok(())
}

impl fmt::Display for Etag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut result = [0u8; 16];
        base64triple(&self.0[..3], &mut result[..4]);
        base64triple(&self.0[3..6], &mut result[4..8]);
        base64triple(&self.0[6..9], &mut result[8..12]);
        base64triple(&self.0[9..], &mut result[12..]);
        write!(f, r#"W/"{}""#, unsafe { from_utf8_unchecked(&result[..]) })
    }
}

impl fmt::Debug for Etag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Etag({})", self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn format() {
        assert_eq!(format!("{}",
            Etag([181, 130, 83, 244, 162, 84, 35, 66, 151, 216, 142, 106])),
            String::from(r#"W/"tYJT9KJUI0KX2I5q""#));
    }
}
