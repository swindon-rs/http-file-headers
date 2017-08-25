Http-file-headers Crate
=======================

[Github](https://github.com/swindon-rs/http-file-headers) |
[Documentaion](http://docs.rs/http-file-headers) |
[Examples](https://github.com/swindon-rs/http-file-headers/tree/master/examples) |
[Crate](https://crates.io/crates/http-file-headers)


A framwork-agnostic library that parses file related headers from HTTP request
and helps serving files including support of:

* `ETag`, `If-None-Match`
* `Last-Modified`, `If-Modified-Since`
* `Accept-Ranges`, `Range`, `Content-Range`
* `Content-Type` using mime_guess_
* `Accept-Encoding` for serving compressed (gzip and brotli) files
* Serving `index.html` or similar directory indexes

The library is not tied to any framework, HTTP or even async library. So
it's usage is quite verbose (see [example][1]). Still it does most of the
complex work internally and is easily adapted to different needs.

Here are just few things that [example in < 200 LoCs][1] shows:

1. [Tokio](https://tokio.rs] asynchronous stuff
2. [Tk-http](https://github.com/swindon-rs/tk-http) serving HTTP
3. [Futures-cpupool](https://crates.io/crates/futures-cpupool) for
    offloading reading from disk into separate thread pool
4. Adding custom headers and error pages
5. Customizing path where file are served from

[1]: https://github.com/swindon-rs/http-file-headers/tree/master/examples/serve_dir.rs


License
=======

Licensed under either of

* Apache License, Version 2.0,
  (./LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license (./LICENSE-MIT or http://opensource.org/licenses/MIT)
  at your option.

Contribution
------------

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

