extern crate fnv;
extern crate portus;
extern crate slog;
extern crate time;

use fnv::FnvHashMap as HashMap;
use portus::ipc::Ipc;
use portus::lang::Scope;
use portus::{CongAlg, Datapath, DatapathInfo, DatapathTrait, Report};
use slog::{debug, info};

pub struct CcpExample {
    logger: Option<slog::Logger>,
    sc: Scope,
    set: CcpExampleConfigEnum,
    mss: u32,
    perack: bool,
}

#[derive(Clone)]
pub enum CcpExampleConfigEnum {
    Rate(u32),
    Cwnd(u32),
}

#[derive(Clone)]
pub struct CcpExampleConfig {
    pub logger: Option<slog::Logger>,
    pub set: CcpExampleConfigEnum,
    pub perack: bool,
}

impl<I: Ipc> CongAlg<I> for CcpExampleConfig {
    type Flow = CcpExample;

    fn name() -> &'static str {
        "flat_rate_cwnd"
    }

    fn datapath_programs(&self) -> HashMap<&'static str, String> {
        let mut h = HashMap::default();
        h.insert(
            "perack",
            "
            (def
                (Report.minrtt +infinity)
                (Report.rtt 0)
                (Report.cwnd 0)
                (Report.rin 0)
                (Report.rout 0)
            )
            (when true
                (:= Report.rtt Flow.rtt_sample_us)
                (:= Report.minrtt Flow.rtt_sample_us)
                (:= Report.cwnd Cwnd)
                (:= Report.rin Flow.rate_outgoing)
                (:= Report.rout Flow.rate_incoming)
                (report)
            )"
            .to_owned(),
        );
        h.insert(
            "interval",
            "
            (def
                (Report.minrtt +infinity)
                (Report.rtt 0)
                (Report.cwnd 0)
                (Report.rin 0)
                (Report.rout 0)
                (interval 100000)
            )
            (when true
                (:= Report.rtt Flow.rtt_sample_us)
                (:= Report.minrtt Flow.rtt_sample_us)
                (:= Report.cwnd Cwnd)
                (:= Report.rin Flow.rate_outgoing)
                (:= Report.rout Flow.rate_incoming)
                (fallthrough)
            )
            (when (> Micros interval)
                (report)
                (reset)
            )"
            .to_owned(),
        );

        h
    }

    fn new_flow(&self, mut control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
        let mut s = CcpExample {
            logger: self.logger.clone(),
            sc: Default::default(),
            set: self.set.clone(),
            mss: info.mss,
            perack: self.perack,
        };

        s.logger.as_ref().map(|log| {
            info!(log, "starting ccp_example_alg flow"; "sock_id" => info.sock_id);
        });

        s.sc = if s.perack {
            self.logger.as_ref().map(|log| {
                debug!(log, "installing perack program");
            });

            control.set_program("perack", None).unwrap()
        } else {
            let interval = time::Duration::milliseconds(100);
            self.logger.as_ref().map(|log| {
                debug!(log, "installing program"; "interval (us)" => interval.num_microseconds().unwrap());
            });

            control
                .set_program(
                    "interval",
                    Some(&[("interval", interval.num_microseconds().unwrap() as u32)]),
                )
                .unwrap()
        };

        match s.set {
            CcpExampleConfigEnum::Cwnd(c) => control.update_field(&s.sc, &[("Cwnd", c)]),
            CcpExampleConfigEnum::Rate(r) => control.update_field(&s.sc, &[("Rate", r)]),
        }
        .unwrap();

        s
    }
}

impl CcpExample {
    fn get_fields(&mut self, m: Report) -> (u32, u32, u32, u32, u32) {
        let sc = &self.sc;
        let minrtt = m
            .get_field("Report.minrtt", sc)
            .expect("expected minrtt field in returned measurement") as u32;

        let rtt = m
            .get_field("Report.rtt", sc)
            .expect("expected rtt field in returned measurement") as u32;

        let cwnd = m
            .get_field("Report.cwnd", sc)
            .expect("expected cwnd field in returned measurement") as u32;

        let rin = m
            .get_field("Report.rin", sc)
            .expect("expected rin field in returned measurement") as u32;

        let rout = m
            .get_field("Report.rout", sc)
            .expect("expected rout field in returned measurement") as u32;

        (minrtt, rtt, cwnd, rin, rout)
    }
}

impl portus::Flow for CcpExample {
    fn on_report(&mut self, _sock_id: u32, m: Report) {
        let (minrtt, rtt, cwnd, rin, rout) = self.get_fields(m);
        self.logger.as_ref().map(|log| {
            debug!(log, "measurement";
                "min_rtt (us)" => minrtt,
                "rtt (us)" => rtt,
                "cwnd (pkts)" => cwnd / self.mss,
                "rin (Bps)" => rin,
                "rout (Bps)" => rout,
            );
        });
    }
}
