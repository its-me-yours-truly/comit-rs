use bam::{config::Config, json::*, *};
use futures::future;

pub fn config() -> Config<Request, Response> {
    Config::default().on_request("PING", &[], |_: Request| {
        Box::new(future::ok(Response::new(Status::OK(0))))
    })
}
