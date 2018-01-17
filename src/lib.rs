#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

use portus::{CongAlg, Config, Datapath, DatapathInfo, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct CcpExample<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    set: CcpExampleConfigEnum,
    mss: u32,
}

#[derive(Clone)]
pub enum CcpExampleConfigEnum {
    Rate(u32),
    Cwnd(u32),
}

#[derive(Clone)]
pub struct CcpExampleConfig {
    pub set: CcpExampleConfigEnum,
}

impl Default for CcpExampleConfig {
    fn default() -> Self {
        CcpExampleConfig {
            set: CcpExampleConfigEnum::Rate(125000) // 1 Mbps
        }
    }
}

impl<T: Ipc> CcpExample<T> {
    fn send_pattern(&self) {
        let pattern = match self.set {
            CcpExampleConfigEnum::Cwnd(c) => {
                self.logger.as_ref().map(|log| {
                    info!(log, "set pattern"; "cwnd" => c);
                });
                make_pattern!(
                    pattern::Event::SetCwndAbs(c) =>
                    pattern::Event::WaitNs(1000000000) => 
                    pattern::Event::Report
                )
            }
            CcpExampleConfigEnum::Rate(r) => {
                self.logger.as_ref().map(|log| {
                    info!(log, "set pattern"; "rate" => r);
                });
                make_pattern!(
                    pattern::Event::SetRateAbsWithCwnd(r) =>
                    pattern::Event::WaitNs(1000000000) => 
                    pattern::Event::Report
                )
            }
        };

        match self.control_channel.send_pattern(self.sock_id, pattern) {
            Ok(_) => (),
            Err(e) => {
                self.logger.as_ref().map(|log| {
                    warn!(log, "send_pattern"; "err" => ?e);
                });
            }
        }
    }

    fn install_fold(&self) -> Option<Scope> {
        match self.control_channel.install_measurement(
            self.sock_id,
            "
                (def (minrtt +infinity) (rtt 0) (then 0) (rin 0) (rout 0))
                (bind Flow.minrtt (min Flow.minrtt Pkt.rtt_sample_us))
                (bind Flow.rtt (ewma 3 Pkt.rtt_sample_us))

                (bind inter 0)
                (bind inter (!if (eq Flow.then 0) (- Pkt.now Flow.then)))

                (bind Flow.rin (if (> inter 0) prev_rin))
                (bind Flow.rout (if (> inter 0) prev_rout))
                (bind prev_rin Pkt.rate_outgoing)
                (bind prev_rout Pkt.rate_incoming)
                (bind Flow.then Pkt.now)
            "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(e) => {
                self.logger.as_ref().map(|log| {
                    warn!(log, "install_measurement"; "err" => ?e);
                });
                None
            }
        }
    }

    fn get_fields(&mut self, m: Measurement) -> (u32, u32, u32, u32, u32) {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let minrtt = m.get_field(&String::from("Flow.minrtt"), sc).expect(
            "expected minrtt field in returned measurement",
        ) as u32;

        let rtt = m.get_field(&String::from("Flow.rtt"), sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;

        let cwnd = m.get_field(&String::from("Cwnd"), sc).expect(
            "expected cwnd field in returned measurement",
        ) as u32;

        let rin = m.get_field(&String::from("Flow.rin"), sc).expect(
            "expected rin field in returned measurement",
        ) as u32;

        let rout = m.get_field(&String::from("Flow.rout"), sc).expect(
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
            sock_id: info.sock_id,
            control_channel: control,
            sc: None,
            logger: cfg.logger,
            set: cfg.config.set,
            mss: info.mss,
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting ccp_example_alg flow"; "sock_id" => info.sock_id);
        });

        s.sc = s.install_fold();
        s.send_pattern();
        s
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
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
