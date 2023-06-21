use std::{
    error::Error,
    fmt,
    net::{SocketAddr, UdpSocket},
    time::{Instant, Duration}, alloc::System,
};
use rand;
use clap::Parser;
use serde_json::json;

/// A simple application emulating a physical input node
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// area name
    #[arg(short, long, required = true)]
    area: String,
    /// flow name
    #[arg(short, long, required = true)]
    flow: String,
    /// The initial target address (<ip>:<port>)
    #[arg(short, long, required = true)]
    target: String,
    /// The outgoing port
    #[arg(short, long, required = true)]
    outgoing_port: u16,
    /// The incoming port
    #[arg(short='i', long, required = true)]
    incoming_port: u16,
    /// data interval (ms)
    #[arg(long, default_value = "1000")]
    interval: u128,
    /// inbound poll interval (ms)
    #[arg(long, default_value = "10")]
    inbound_poll_interval: u128,
}

pub fn main() -> () {

    let args = Args::parse();

    println!("Starting input node for flow '{}' @ area '{}'", args.flow, args.area);

    let target = SocketAddr::from(args.target.parse::<SocketAddr>().expect("No valid target address given. Use format: <ip>:<port>"));

    let outbound_socket = UdpSocket::bind(format!("127.0.0.1:{}", args.outgoing_port)).expect("Couldn't bind outbound socket");

    let inbound_socket = UdpSocket::bind(format!("127.0.0.1:{}", args.incoming_port)).expect("Couldn't bind inbound socket");
    let timeout = Duration::from_millis(10);
    inbound_socket.set_read_timeout(timeout.into()).expect("Couldn't set socket timeout");


    let mut timer_generate_data = Instant::now();
    let mut timer_check_incoming = timer_generate_data.clone();

    let mut buf = [0; 1024];

    loop {

        if timer_check_incoming.elapsed().as_millis() > args.inbound_poll_interval {

            // check socket for incoming data
            if let Ok((message_length, src)) = inbound_socket.recv_from(&mut buf) {
                // convert to string
                let message = String::from_utf8(buf[..message_length].into()).expect("Couldn't convert to String");
                println!("Received data from {}: {}", src, message);
    
            } else {
                // no data received
                // println!("No data received")
            }

            // reset timer
            timer_check_incoming = Instant::now();
        }
        
        if timer_generate_data.elapsed().as_millis() > args.interval {

            let data = generate_input_data();

            let json = json!({
                "message": data,
                "meta": {
                    "flow_name": args.flow,
                    "execution_area": args.area
                }
            });
            
            println!("Sending data to {}: {}", target, data);
            outbound_socket.send_to(&json.to_string().as_bytes(), target).expect("Couldn't send data");

            // reset timer
            timer_generate_data = Instant::now();
        }
        
    }

}

fn generate_input_data() -> u64 {
    // generate a random number
    let random_number = rand::random::<u64>();
    random_number
}
