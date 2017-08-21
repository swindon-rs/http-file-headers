extern crate futures;
extern crate futures_cpupool;
extern crate tk_http;
extern crate tk_http_file;
extern crate tk_listen;
extern crate tokio_core;
#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;

use std::io;
use std::time::Duration;
use std::path::{Path, PathBuf};
use std::ffi::OsString;
use std::fs::File;

use futures::{Future, Stream, Async};
use futures::future::{ok, FutureResult, Either};
use futures_cpupool::{CpuPool, CpuFuture};
use tk_listen::ListenExt;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tk_http::server;
use tk_http::Status;
use tk_http_file::{Input, Output};

const MAX_SIMULTANEOUS_CONNECTIONS: usize = 500;
const TIME_TO_WAIT_ON_ERROR: u64 = 100;

lazy_static! {
    static ref POOL: CpuPool = CpuPool::new(8);
}

type ResponseFuture<S> = Box<Future<Item=server::EncoderDone<S>,
                                   Error=server::Error>>;

struct Codec {
    fut: Option<CpuFuture<Option<(File, Output)>, Status>>,
}

struct Dispatcher {
}

fn respond_error<S: 'static>(status: Status, mut e: server::Encoder<S>)
    -> FutureResult<server::EncoderDone<S>, server::Error>
{
    let body = format!("{} {}", status.code(), status.reason());
    e.status(status);
    e.add_length(body.as_bytes().len() as u64).unwrap();
    if e.done_headers().unwrap() {
        e.write_body(body.as_bytes());
    }
    ok(e.done())
}

impl<S: 'static> server::Codec<S> for Codec {
    type ResponseFuture = ResponseFuture<S>;
    fn recv_mode(&mut self) -> server::RecvMode {
        server::RecvMode::buffered_upfront(0)
    }
    fn data_received(&mut self, data: &[u8], end: bool)
        -> Result<Async<usize>, server::Error>
    {
        debug_assert!(end && data.len() == 0);
        Ok(Async::Ready(0))
    }
    fn start_response(&mut self, mut e: server::Encoder<S>)
        -> Self::ResponseFuture
    {
        Box::new(self.fut.take().unwrap().then(move |result| {
            match result {
                Ok(Some((f, outp))) => {
                    e.status(Status::Ok);
                    e.add_length(outp.content_length()).unwrap();
                    for (name, val) in outp.headers() {
                        e.format_header(name, val).unwrap();
                    }
                    // add headers
                    if e.done_headers().unwrap() {
                        // start writing body
                        Either::B(loop_fn(outp, |outp| {
                            POOL.spawn_fn(|| {
                                
                            })
                        }))
                    } else {
                        Either::A(ok(e.done()))
                    }
                }
                Ok(None) => {
                    Either::A(respond_error(Status::NotFound, e))
                }
                Err(status) => {
                    Either::A(respond_error(status, e))
                }
            }
        }))
    }
}

impl<S: 'static> server::Dispatcher<S> for Dispatcher {
    type Codec = Codec;
    fn headers_received(&mut self, head: &server::Head)
        -> Result<Self::Codec, server::Error>
    {
        let inp = Input::from_headers(head.method(), head.headers());
        let path = Path::new("./public").join(head.path()
            .expect("only static requests expected") // fails on OPTIONS
            .trim_left_matches(|x| x == '/'));
        let fut = POOL.spawn_fn(move || {
            // TODO(tailhook) move this expansion to a library
            Ok(inp.file_at(path))
        });
        Ok(Codec {
            fut: Some(fut),
        })
    }
}


fn main() {
    let addr = "127.0.0.1:8000".parse().unwrap();

    let mut lp = Core::new().unwrap();
    let h1 = lp.handle();
    let listener = TcpListener::bind(&addr, &lp.handle()).unwrap();
    let cfg = server::Config::new().done();
    println!("Listening on {}", addr);
    lp.run(
        listener.incoming()
        .sleep_on_error(Duration::from_millis(TIME_TO_WAIT_ON_ERROR), &h1)
        .map(move |(mut socket, _addr)| {
            server::Proto::new(socket, &cfg, Dispatcher {}, &h1)
            .map_err(|e| { println!("Connection error: {}", e); })
        })
        .listen(MAX_SIMULTANEOUS_CONNECTIONS)
    ).unwrap(); // stream doesn't end in this case
}
