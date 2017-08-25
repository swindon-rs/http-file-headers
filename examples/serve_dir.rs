extern crate futures;
extern crate futures_cpupool;
extern crate tk_http;
extern crate tk_http_file;
extern crate tk_listen;
extern crate tokio_core;
extern crate tokio_io;
#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;

use std::time::Duration;
use std::path::{Path};
use std::sync::Arc;

use futures::{Future, Stream, Async};
use futures::future::{ok, FutureResult, Either, loop_fn, Loop};
use futures_cpupool::{CpuPool, CpuFuture};
use tk_listen::ListenExt;
use tokio_io::AsyncWrite;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tk_http::server;
use tk_http::Status;
use tk_http_file::{Input, Output, Config};

const MAX_SIMULTANEOUS_CONNECTIONS: usize = 500;
const TIME_TO_WAIT_ON_ERROR: u64 = 100;

lazy_static! {
    static ref POOL: CpuPool = CpuPool::new(8);
    static ref CONFIG: Arc<Config> = Config::new()
        .add_index_file("index.html")
        .done();
}

type ResponseFuture<S> = Box<Future<Item=server::EncoderDone<S>,
                                   Error=server::Error>>;

struct Codec {
    fut: Option<CpuFuture<Output, Status>>,
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

impl<S: AsyncWrite + Send + 'static> server::Codec<S> for Codec {
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
                Ok(Output::File(outf)) | Ok(Output::FileRange(outf)) => {
                    if outf.is_partial() {
                        e.status(Status::PartialContent);
                    } else {
                        e.status(Status::Ok);
                    }
                    e.add_length(outf.content_length()).unwrap();
                    for (name, val) in outf.headers() {
                        e.format_header(name, val).unwrap();
                    }
                    // add headers
                    if e.done_headers().unwrap() {
                        // start writing body
                        Either::B(loop_fn((e, outf), |(mut e, mut outf)| {
                            POOL.spawn_fn(move || {
                                outf.read_chunk(&mut e).map(|b| (b, e, outf))
                            }).and_then(|(b, e, outf)| {
                                e.wait_flush(4096).map(move |e| (b, e, outf))
                            }).map(|(b, e, outf)| {
                                if b == 0 {
                                    Loop::Break(e.done())
                                } else {
                                    Loop::Continue((e, outf))
                                }
                            }).map_err(|e| server::Error::custom(e))
                        }))
                    } else {
                        Either::A(ok(e.done()))
                    }
                }
                Ok(Output::FileHead(head)) | Ok(Output::NotModified(head)) => {
                    if head.is_not_modified() {
                        e.status(Status::NotModified);
                    } else if head.is_partial() {
                        e.status(Status::PartialContent);
                        e.add_length(head.content_length()).unwrap();
                    } else {
                        e.status(Status::Ok);
                        e.add_length(head.content_length()).unwrap();
                    }
                    for (name, val) in head.headers() {
                        e.format_header(name, val).unwrap();
                    }
                    assert_eq!(e.done_headers().unwrap(), false);
                    Either::A(ok(e.done()))
                }
                Ok(Output::InvalidRange) => {
                    Either::A(respond_error(
                        Status::RequestRangeNotSatisfiable, e))
                }
                Ok(Output::InvalidMethod) => {
                    Either::A(respond_error(
                        Status::MethodNotAllowed, e))
                }
                Ok(Output::NotFound) | Ok(Output::Directory) => {
                    Either::A(respond_error(Status::NotFound, e))
                }
                Err(status) => {
                    Either::A(respond_error(status, e))
                }
            }
        }))
    }
}

impl<S: AsyncWrite + Send + 'static> server::Dispatcher<S> for Dispatcher {
    type Codec = Codec;
    fn headers_received(&mut self, head: &server::Head)
        -> Result<Self::Codec, server::Error>
    {
        let inp = Input::from_headers(&*CONFIG, head.method(), head.headers());
        let path = Path::new("./public").join(head.path()
            .expect("only static requests expected") // fails on OPTIONS
            .trim_left_matches(|x| x == '/'));
        let fut = POOL.spawn_fn(move || {
            inp.probe_file(&path).map_err(|e| {
                error!("Error reading file {:?}: {}", path, e);
                Status::InternalServerError
            })
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
        .map(move |(socket, _addr)| {
            server::Proto::new(socket, &cfg, Dispatcher {}, &h1)
            .map_err(|e| { println!("Connection error: {}", e); })
        })
        .listen(MAX_SIMULTANEOUS_CONNECTIONS)
    ).unwrap(); // stream doesn't end in this case
}
