//#[warn(missing_docs)]
#![allow(dead_code)]

extern crate httpdate;
#[macro_use] extern crate log;

mod input;
mod output;
mod accept_encoding;

pub use input::Input;
pub use output::Output;
pub use accept_encoding::{AcceptEncoding, Encoding, Iter as EncodingIter};
