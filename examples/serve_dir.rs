extern crate futures;
extern crate tk_listen;
extern crate tokio_core;
#[macro_use] extern crate log;

use std::time::Duration;

use futures::Stream;
use tk_listen::ListenExt;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;


fn main() {
    let TIME_TO_WAIT_ON_ERROR = Duration::from_millis(100);
    let MAX_SIMULTANEOUS_CONNECTIONS = 500;
    let addr = "127.0.0.1:8000".parse().unwrap();

    let mut lp = Core::new().unwrap();
    let h2 = lp.handle();
    let listener = TcpListener::bind(&addr, &lp.handle()).unwrap();
    println!("Listening on {}", addr);
    lp.run(
        listener.incoming()
        .sleep_on_error(TIME_TO_WAIT_ON_ERROR, &h2)
        .map(move |(mut socket, _addr)| {
            unimplemented!();
            Ok(())
        })
        .listen(MAX_SIMULTANEOUS_CONNECTIONS)
    ).unwrap(); // stream doesn't end in this case
}
