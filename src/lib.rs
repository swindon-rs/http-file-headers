//! A helper library for serving files over HTTP
//!
//! See [examples][1] for full example of how to use it.
//!
//! [Github](https://github.com/swindon-rs/http-file-headers) |
//! [Examples][1] |
//! [Crate](https://crates.io/crates/http-file-headers)
//!
//! [1]: https://github.com/swindon-rs/http-file-headers/tree/master/examples
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

extern crate blake2;
extern crate byteorder;
extern crate digest_writer;
extern crate generic_array;
extern crate httpdate;
extern crate mime_guess;
extern crate typenum;

mod conditionals;
mod config;
mod etag;
mod input;
mod output;
mod range;
mod accept_encoding;

pub use input::Input;
pub use config::Config;
pub use output::{Output, Head, FileWrapper};
pub use accept_encoding::{Encoding, Iter as EncodingIter};
