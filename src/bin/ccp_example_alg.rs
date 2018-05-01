extern crate clap;

#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
use slog::Drain;

extern crate ccp_example_alg;
extern crate portus;

use clap::Arg;
use ccp_example_alg::CcpExample;
use portus::ipc::{BackendBuilder, ListenMode};

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

fn make_args() -> Result<(ccp_example_alg::CcpExampleConfig, String), String> {
    let matches = clap::App::new("CCP Constant Cwnd/Rate Example")
        .version("0.1.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Example congestion control algorithm which sets a constant rate or delay")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix)")
             .takes_value(true)
             .required(true)
             .validator(portus::algs::ipc_valid))
        .arg(Arg::with_name("cwnd")
             .long("cwnd")
             .takes_value(true)
             .help("Sets the congestion window, in bytes."))
        .arg(Arg::with_name("rate")
             .long("rate")
             .takes_value(true)
             .help("Sets the rate to use, in bytes / second"))
        .group(clap::ArgGroup::with_name("to_set")
               .args(&["cwnd", "rate"])
               .required(true))
        .arg(Arg::with_name("report_per_ack")
             .long("per_ack")
             .help("Specifies that the datapath should send a measurement upon every ACK"))
        .get_matches();

    if matches.is_present("to_set") {
        if matches.is_present("rate") {
            let rate = u32::from_str_radix(matches.value_of("rate").unwrap(), 10).map_err(|e| format!("{:?}", e))?;
            Ok((
                ccp_example_alg::CcpExampleConfig {
                    set: ccp_example_alg::CcpExampleConfigEnum::Rate(rate),
                    perack: matches.is_present("report_per_ack"),
                },
                String::from(matches.value_of("ipc").unwrap()),
            ))
        } else if matches.is_present("cwnd") {
            let cwnd = u32::from_str_radix(matches.value_of("cwnd").unwrap(), 10).map_err(|e| format!("{:?}", e))?;
            Ok((
                ccp_example_alg::CcpExampleConfig {
                    set: ccp_example_alg::CcpExampleConfigEnum::Cwnd(cwnd),
                    perack: matches.is_present("report_per_ack"),
                },
                String::from(matches.value_of("ipc").unwrap()),
            ))
        } else {
            Err(String::from("must specify rate or cwnd"))
        }
    } else {
        Err(String::from("must specify rate or cwnd"))
    }
}

fn main() {
    let log = make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap();

    info!(log, "starting CCP Example");
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out")
                .map(|sk| BackendBuilder {sock: sk,  mode: ListenMode::Blocking})
                .expect("ipc initialization");
            portus::run::<_, CcpExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
                ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::new()
                .map(|sk| BackendBuilder {sock: sk,  mode: ListenMode::Blocking})
                .expect("ipc initialization");
            portus::run::<_, CcpExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
                ).unwrap();
        }
        _ => unreachable!(),
    }
}
