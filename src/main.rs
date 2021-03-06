use tokio::prelude::*;
use tokio::timer::Interval;
use tokio::timer::Delay;
use tokio::runtime::Runtime;
use lazy_static::lazy_static;

use std::cmp::min;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const VERSION: &str = "v0.1.0";
const NAME: &str = "ictmon";
const PORT: u16 = 5560;
const MOVING_AVG_INTERVAL_MS: u64 = 60000;
const INITIAL_SLEEP_MS: u64 = 1000;
const UPDATE_INTERVAL_MS: u64 = 1000;

lazy_static! {
    static ref ADDRESS: String = String::from("localhost");
    static ref CHANNEL_TX: String = String::from("tx");
}

struct Arguments {
    address: String,
    port: u16,
}

impl Arguments {
    pub fn new(args: Vec<String>) -> Result<Self, String> {
        match args.len() {
            1 => Ok(Arguments {
                address: ADDRESS.clone(),
                port: PORT,
            }),
            3 => Ok(Arguments {
                address: args[1].clone(),
                port: args[2].parse::<u16>().unwrap(),
            }),
            _ => Err(format!(
                "Wrong number of arguments provided. Usage: ./{} <IP> <ZMQ-Port>",
                NAME
            )),
        }
    }
}

fn main() {
    let args: Arguments = match Arguments::new(env::args().collect::<Vec<String>>()) {
        Ok(a) => a,
        Err(s) => {
            println!("{}", s);
            process::exit(0);
        }
    };

    println!("Welcome to '{}' (Ict Network Monitor) {}", NAME, VERSION);

    let context = zmq::Context::new();
    let subscriber = context.socket(zmq::SUB).unwrap();
    let address = format!("tcp://{}:{}", args.address, args.port);

    subscriber.connect(&address).unwrap_or_else(|_| {
        panic!(
            "Could not connect to publisher: '{}:{}'.",
            args.address, args.port
        )
    });

    println!(
        "Info: Subscribed to Ict node running ZeroMQ IXI extension module at '{}:{}'.",
        args.address, args.port
    );

    println!("\n");

    let subscription = CHANNEL_TX.as_bytes();
    subscriber.set_subscribe(&subscription).unwrap();

    let arrival_timestamps: Arc<Mutex<VecDeque<Instant>>> = Arc::new(Mutex::new(VecDeque::new()));
    let arrival_timestamps_recv = Arc::clone(&arrival_timestamps);

    let poller = Delay::new(Instant::now())
    .and_then(move |_| {
        let mut arrival_timestamp: Instant;
        loop {
            subscriber.recv_msg(0).unwrap();
            arrival_timestamp = Instant::now();

            let mut queue = arrival_timestamps_recv.lock().unwrap();
            queue.push_back(arrival_timestamp);
        }
        Ok(())
    })
    .map_err(|e| panic!("interval errored; err={:?}", e));


    let interval = Duration::from_millis(MOVING_AVG_INTERVAL_MS);

    let mut uptime_ms: u64 = 0;
    let init = Instant::now();

    thread::sleep(Duration::from_millis(INITIAL_SLEEP_MS));

    let interval_task = Interval::new_interval(Duration::from_millis(UPDATE_INTERVAL_MS))
        .for_each(move |instant| {
            let window_start = instant - interval;
            {
                let mut queue = arrival_timestamps.lock().unwrap();
                while queue.len() > 0 && queue.front().unwrap() < &window_start {
                    queue.pop_front();
                }
                uptime_ms = (instant - init).as_secs() * 1000 + u64::from((instant - init).subsec_millis());
                print_tps(
                    queue.len() as f64 / (min(MOVING_AVG_INTERVAL_MS, uptime_ms) as f64 / 1000_f64),
                );
            }
            Ok(())
        })
        .map_err(|e| panic!("interval errored; err={:?}", e));
    
    let mut rt = Runtime::new().unwrap();
    rt.spawn(poller);
    rt.spawn(interval_task);
    rt.shutdown_on_idle().wait().unwrap();
}

fn print_tps(tps: f64) {
    print!(
        "\r\x1b[2A+--------------+\n|{:>10.2} tps|\n+--------------+",
        tps
    );
    io::stdout().flush().unwrap();
}
