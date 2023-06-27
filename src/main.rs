use std::{
    error::Error,
    fmt,
    net::{SocketAddr, UdpSocket},
    time::{Instant, Duration}, alloc::System,
};
use rand;
use clap::Parser;
use serde_json::json;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Config {
    pub area: String,
    pub flow_name: String,
    pub target_ip: String,
    pub target_port: u16,
    pub outbound_port: u16,
    pub inbound_port: u16,
    pub interval: Option<u128>,
    pub inbound_poll_interval: Option<u128>,
}

/// A simple application emulating a physical input node
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// area name
    #[arg(short, long)]
    area: Option<String>,
    /// flow name
    #[arg(short, long)]
    flow: Option<String>,
    /// The initial target ip
    #[arg(short='t', long)]
    target_ip:Option< String>,
    /// The initial target port
    #[arg(short='p', long)]
    target_port: Option<u16>,
    /// The outgoing port
    #[arg(short, long)]
    outbound_port: Option<u16>,
    /// The incoming port
    #[arg(short='i', long)]
    inbound_port: Option<u16>,
    /// data interval (ms)
    #[arg(long, default_value = "1000")]
    interval: u128,
    /// inbound poll interval (ms)
    #[arg(long, default_value = "10")]
    inbound_poll_interval: u128,
    /// config file
    #[arg(short, long)]
    config: Option<String>,
}

pub fn main() -> () {

    let args = Args::parse();
    let config: Config;

    match args.config {
        Some(config_path) => {
            match load_config(config_path.as_str()).as_mut() {
                Ok(loaded_config) => {
                    match loaded_config.interval {
                        None => {
                            loaded_config.interval = Some(args.interval);
                        },
                        _ => {},
                    }
                    match loaded_config.inbound_poll_interval {
                        None => {
                            loaded_config.inbound_poll_interval = Some(args.inbound_poll_interval);
                        },
                        _ => {},
                    }
                    
                    config = loaded_config.clone();
                    println!("Config loaded: {:?}", config);
                    
                },
                Err(e) => {
                    panic!("Couldn't load config: {}", e);
                }
            }
        },
        None => {
            // create config from args
            config = Config{
                area: args.area.expect("Argument `area` is required unless a config file is specified!").clone(),
                flow_name: args.flow.expect("Argument `flow` is required unless a config file is specified!").clone(),
                target_ip: args.target_ip.expect("Argument `target_ip` is required unless a config file is specified!").clone(),
                target_port: args.target_port.expect("Argument `target_port` is required unless a config file is specified!"),
                outbound_port: args.outbound_port.expect("Argument `outbound_port` is required unless a config file is specified!"),
                inbound_port: args.inbound_port.expect("Argument `inbound_port` is required unless a config file is specified!"),
                interval: Some(args.interval),
                inbound_poll_interval: Some(args.inbound_poll_interval),
            };
        }
    }

    println!("Starting input node for flow '{}' @ area '{}'", config.flow_name, config.area);

    let mut target = SocketAddr::from(format!("{}:{}", config.target_ip, config.target_port).parse::<SocketAddr>().expect("No valid target address given. Use format: <ip>:<port>"));

    let outbound_socket = UdpSocket::bind(format!("0.0.0.0:{}", config.outbound_port)).expect("Couldn't bind outbound socket");

    let inbound_socket = UdpSocket::bind(format!("0.0.0.0:{}", config.inbound_port)).expect("Couldn't bind inbound socket");
    let timeout = Duration::from_millis(10);
    inbound_socket.set_read_timeout(timeout.into()).expect("Couldn't set socket timeout");


    let mut timer_generate_data = Instant::now();
    let mut timer_check_incoming = timer_generate_data.clone();

    let mut buf = [0; 1024];

    loop {

        if timer_check_incoming.elapsed().as_millis() > config.inbound_poll_interval.unwrap() {

            // check socket for incoming data
            if let Ok((message_length, src)) = inbound_socket.recv_from(&mut buf) {
                // convert to string
                let message = String::from_utf8(buf[..message_length].into()).expect("Couldn't convert to String");
                println!("Received data from {}: {}", src, message);

                // parse json
                let json: serde_json::Value = serde_json::from_str(&message).expect("Couldn't parse JSON");
                if let Some(message_type) = json["type"].as_str() {
                    if message_type == "updateTarget" {

                        // take 10k part from the new target port and fill the rest with the old one
                        let new_target_port_base = json["target_port_base"].as_u64().expect("No target base port given") as u16;
                        let new_target_port = new_target_port_base + ((config.target_port % 10000) as u16);
                        println!("New target port: {}", new_target_port);

                        let new_target_address_string = format!("{}:{}", json["target"].as_str().expect("No target ip given"), new_target_port);
                        let new_target_address = SocketAddr::from(new_target_address_string.parse::<SocketAddr>().expect(format!("Target not updated because target address was invalid: {}", new_target_address_string).as_str()));
                        target = new_target_address;
                        
                        // acknowledge
                        let data = generate_input_data();
                        let json = json!({
                            "type": "updateTarget",
                            "success": true,
                        });
                        println!("Sending ACK to {}: {}", src, json.to_string());
                        // send 10 times to "make sure" it arrives
                        for _ in 0..10 {
                            outbound_socket.send_to(&json.to_string().as_bytes(), src).expect("Couldn't send ACK");
                        }
                    }
                } 
    
            } else {
                // no data received
                // println!("No data received")
            }

            // reset timer
            timer_check_incoming = Instant::now();
        }
        
        if timer_generate_data.elapsed().as_millis() > config.interval.unwrap() {

            let data = generate_input_data();

            let json = json!({
                "message": data.to_string(),
                "meta": {
                    "flow_name": config.flow_name,
                    "execution_area": config.area
                }
            });
            
            println!("Sending data to {}: {}", target, data);
            outbound_socket.send_to(&json.to_string().as_bytes(), target).expect("Couldn't send data");

            // reset timer
            timer_generate_data = Instant::now();
        }
        
    }

}

fn generate_input_data() -> u16 {
    // generate a random number
    let random_number = rand::random::<u16>();
    random_number
}

fn load_config(path: &str) -> Result<Config, Box<dyn Error>> {
    let config = std::fs::read_to_string(path)?;

    let config = serde_yaml::from_str::<Config>(&config);

    println!("config: {:?}", config);

    config.map_err(|err| err.into())
}
