extern crate futures;
extern crate tk_http;
extern crate tk_listen;
extern crate tokio_core;
#[macro_use] extern crate log;

use std::time::Duration;

use futures::{Future, Stream, Async};
use futures::future::ok;
use tk_listen::ListenExt;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tk_http::server;
use tk_http::Status;

const MAX_SIMULTANEOUS_CONNECTIONS: usize = 500;
const TIME_TO_WAIT_ON_ERROR: u64 = 100;

type ResponseFuture<S> = Box<Future<Item=server::EncoderDone<S>,
                                   Error=server::Error>>;

struct Codec {
}

struct Dispatcher {
}

fn respond_error<S: 'static>(status: Status, mut e: server::Encoder<S>)
    -> ResponseFuture<S>
{
    let body = format!("{} {}", status.code(), status.reason());
    e.status(status);
    e.add_length(body.as_bytes().len() as u64).unwrap();
    if e.done_headers().unwrap() {
        e.write_body(body.as_bytes());
    }
    Box::new(ok(e.done()))
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
    fn start_response(&mut self, e: server::Encoder<S>)
        -> Self::ResponseFuture
    {
        respond_error(Status::NotFound, e)
    }
}

impl<S: 'static> server::Dispatcher<S> for Dispatcher {
    type Codec = Codec;
    fn headers_received(&mut self, headers: &server::Head)
        -> Result<Self::Codec, server::Error>
    {

        Ok(Codec {})
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
