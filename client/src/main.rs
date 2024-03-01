use std::{net::Ipv4Addr, thread, time::Duration};

use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use http_body_util::Empty;
use hyper::{body::Bytes, Uri};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client as HyperClient},
    rt::TokioExecutor,
};
// use metrics::{histogram, increment_counter};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use structopt::StructOpt;
use tokio::{
    select,
    sync::watch::{channel, Receiver, Sender},
    task::spawn_blocking,
    time::Instant,
};

#[derive(Debug, StructOpt)]
struct Args {
    host: Uri,
}

#[tokio::main]
async fn main() {
    let args = Args::from_args();

    PrometheusBuilder::new()
        .with_http_listener((Ipv4Addr::LOCALHOST, 9001))
        .set_buckets(&[0.0, 0.01, 0.03, 0.1, 0.3, 1.0, 3.0])
        .unwrap()
        .install()
        .unwrap();

    let client = Client::new(args.host);

    let (rps_sender, rps_receiver) = channel(0u64);

    let user_input = spawn_blocking({
        let client = client.clone();
        move || user_input(rps_sender, client)
    });
    let load = load(rps_receiver, client);
    select! {
        _ = user_input => {},
        _ = load => panic!("load stopped"),
    }
}

#[derive(Debug, Clone)]
struct Client {
    client: HyperClient<HttpConnector, Empty<Bytes>>,
    uri: Uri,
}

impl Client {
    fn new(uri: Uri) -> Self {
        Self {
            client: HyperClient::builder(TokioExecutor::new()).build_http(),
            uri,
        }
    }

    fn send_req(&self) {
        println!("send request");
        let req = self.client.get(self.uri.clone());
        tokio::spawn(async move {
            let start = Instant::now();
            let resp = req.await;
            let latency = start.elapsed();
            // histogram!("client.latency", latency.as_secs_f64());
            histogram!("client.latency").record(latency.as_secs_f64());
            println!("client.latency: {}", latency.as_secs_f64());
            println!("client resp {:?}", resp);
            let status = match resp {
                Ok(resp) => resp.status().as_u16().to_string(),
                Err(_) => "errored".to_string(),
            };
            println!("increment counter client.response: {}", status);
            // increment_counter!("client.response", "status" => status);
            counter!("client.response", "status" => status).increment(1);
        });
    }
}

fn user_input(rps: Sender<u64>, client: Client) {
    loop {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .item("Set load")
            .item("Burst")
            .item("Single request")
            .item("Quit")
            .default(0)
            .interact()
            .unwrap();
        match selection {
            0 => {
                rps.send(
                    Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Requests per second:")
                        .interact_text()
                        .unwrap(),
                )
                .unwrap();
            }
            1 => {
                let old_rps = *rps.borrow();
                let burst_rps = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Requests per second:")
                    .interact_text()
                    .unwrap();
                let time = Duration::from_millis(
                    Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Burst duration [ms]:")
                        .interact_text()
                        .unwrap(),
                );
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Send a burst at {} rps for {}ms",
                        burst_rps,
                        time.as_millis()
                    ))
                    .interact()
                    .unwrap()
                {
                    rps.send(burst_rps).unwrap();
                    thread::sleep(time);
                    rps.send(old_rps).unwrap();
                }
            }
            2 => {
                client.send_req();
            }
            3 => break,
            _ => continue,
        }
    }
}

async fn load(mut rps: Receiver<u64>, client: Client) {
    loop {
        let start = Instant::now();
        let rate = *rps.borrow();
        if rate == 0 {
            rps.changed().await.unwrap();
            continue;
        }

        client.send_req();
        tokio::time::sleep_until(start + Duration::from_nanos(1_000_000_000 / rate)).await;
    }
}
