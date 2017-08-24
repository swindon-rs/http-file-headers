use std::io::Write;
use std::fs::Metadata;
use std::fmt;
use std::time::{Duration, UNIX_EPOCH};
use std::str::from_utf8_unchecked;

use blake2::{Blake2b, Digest};
use digest_writer::Writer;
use typenum::U12;
use byteorder::{WriteBytesExt, BigEndian};


pub struct Etag([u8; 12]);


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
