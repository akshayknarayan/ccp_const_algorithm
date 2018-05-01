extern crate portus;
#[macro_use]
extern crate slog;
extern crate time;

use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct CcpExample<T: Ipc> {
    control_channel: Datapath<T>,
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
    pub set: CcpExampleConfigEnum,
    pub perack: bool,
}

impl Default for CcpExampleConfig {
    fn default() -> Self {
        CcpExampleConfig {
            set: CcpExampleConfigEnum::Rate(125000), // 1 Mbps
            perack: false,
        }
    }
}

impl<T: Ipc> CcpExample<T> {
    fn install_perack(&self) -> Scope {
        self.control_channel.install(b"
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
        ).unwrap()
    }
    
    fn install_interval(&self, interval: time::Duration) -> Scope {
        self.logger.as_ref().map(|log| {
            debug!(log, "installing program"; "interval (us)" => interval.num_microseconds().unwrap());
        });

        self.control_channel.install(format!("
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
                (fallthrough)
            )
            (when (> Micros {})
                (report)
                (reset)
            )", interval.num_microseconds().unwrap()).as_bytes()
        ).unwrap()
    }

    fn get_fields(&mut self, m: Report) -> (u32, u32, u32, u32, u32) {
        let sc = &self.sc;
        let minrtt = m.get_field("Report.minrtt", sc).expect(
            "expected minrtt field in returned measurement",
        ) as u32;

        let rtt = m.get_field("Report.rtt", sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;

        let cwnd = m.get_field("Report.cwnd", sc).expect(
            "expected cwnd field in returned measurement",
        ) as u32;

        let rin = m.get_field("Report.rin", sc).expect(
            "expected rin field in returned measurement",
        ) as u32;

        let rout = m.get_field("Report.rout", sc).expect(
            "expected rout field in returned measurement",
        ) as u32;

        (minrtt, rtt, cwnd, rin, rout)
    }
}

impl<T: Ipc> CongAlg<T> for CcpExample<T> {
    type Config = CcpExampleConfig;

    fn name() -> String {
        String::from("ccp_example")
    }

    fn create(control: Datapath<T>, cfg: Config<T, CcpExample<T>>, info: DatapathInfo) -> Self {
        let mut s = Self {
            control_channel: control,
            sc: Default::default(),
            logger: cfg.logger,
            set: cfg.config.set,
            mss: info.mss,
            perack: cfg.config.perack,
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting ccp_example_alg flow"; "sock_id" => info.sock_id);
        });

        if s.perack {
            s.sc = s.install_perack();
        } else {
            s.sc = s.install_interval(time::Duration::milliseconds(100));
        }

        match s.set {
            CcpExampleConfigEnum::Cwnd(c) => s.control_channel.update_field(&s.sc, &[("Cwnd", c)]),
            CcpExampleConfigEnum::Rate(r) => s.control_channel.update_field(&s.sc, &[("Rate", r)]),
        }.unwrap();

        s
    }

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
