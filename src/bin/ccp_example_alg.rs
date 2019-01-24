extern crate clap;

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;
use slog::Drain;

extern crate ccp_example_alg;
extern crate portus;

use ccp_example_alg::CcpExampleConfig;
use clap::Arg;

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

fn make_args(log: slog::Logger) -> Result<(CcpExampleConfig, String), String> {
    let matches = clap::App::new("CCP Constant Cwnd/Rate Example")
        .version("0.2.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Example congestion control algorithm which sets a constant rate or delay")
        .arg(
            Arg::with_name("ipc")
                .long("ipc")
                .help("Sets the type of ipc to use: (netlink|unix)")
                .takes_value(true)
                .required(true)
                .validator(portus::algs::ipc_valid),
        )
        .arg(
            Arg::with_name("cwnd")
                .long("cwnd")
                .takes_value(true)
                .help("Sets the congestion window, in bytes."),
        )
        .arg(
            Arg::with_name("rate")
                .long("rate")
                .takes_value(true)
                .help("Sets the rate to use, in bytes / second"),
        )
        .group(
            clap::ArgGroup::with_name("to_set")
                .args(&["cwnd", "rate"])
                .required(true),
        )
        .arg(
            Arg::with_name("report_per_ack")
                .long("per_ack")
                .help("Specifies that the datapath should send a measurement upon every ACK"),
        )
        .get_matches();

    if !matches.is_present("to_set") {
        return Err(String::from("must specify rate or cwnd"));
    }

    let set = if matches.is_present("rate") {
        let rate = u32::from_str_radix(matches.value_of("rate").unwrap(), 10)
            .map_err(|e| format!("{:?}", e))?;
        Ok(ccp_example_alg::CcpExampleConfigEnum::Rate(rate))
    } else if matches.is_present("cwnd") {
        let cwnd = u32::from_str_radix(matches.value_of("cwnd").unwrap(), 10)
            .map_err(|e| format!("{:?}", e))?;
        Ok(ccp_example_alg::CcpExampleConfigEnum::Cwnd(cwnd))
    } else {
        Err(String::from("must specify rate or cwnd"))
    }?;

    Ok((
        CcpExampleConfig {
            logger: Some(log.clone()),
            set,
            perack: matches.is_present("report_per_ack"),
        },
        String::from(matches.value_of("ipc").unwrap()),
    ))
}

fn main() {
    let log = make_logger();
    let (cfg, ipc) = make_args(log.clone())
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap();

    info!(log, "starting CCP Example");
    portus::start!(ipc.as_str(), Some(log), cfg).unwrap()
}
