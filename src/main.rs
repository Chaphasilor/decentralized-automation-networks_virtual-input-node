use std::{
    error::Error,
    net::{SocketAddr},
    time::{Duration}, sync::Arc,
};
use rand;
use clap::Parser;
use serde_json::json;
use tokio::{net::UdpSocket, time};
use std::sync::Mutex;
use futures::{future};

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Config {
    pub area: String,
    pub flow_name: String,
    pub target_ip: String,
    pub target_port: u16,
    pub outbound_port_data: u16,
    pub outbound_port_acks: Option<u16>,
    pub inbound_port: u16,
    pub interval: Option<u64>,
    pub inbound_poll_interval: Option<u64>,
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
    /// The outgoing port for sending data
    #[arg(short, long)]
    outbound_port_data: Option<u16>,
    /// The outgoing port for sending ACKs
    #[arg(long, default_value = "0")]
    outbound_port_acks: u16,
    /// The incoming port
    #[arg(short='i', long)]
    inbound_port: Option<u16>,
    /// data interval (ms)
    #[arg(long, default_value = "1000")]
    interval: u64,
    /// inbound poll interval (ms)
    #[arg(long, default_value = "10")]
    inbound_poll_interval: u64,
    /// config file
    #[arg(short, long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> () {

    let args = Args::parse();
    let config: Config;

    match args.config {
        Some(config_path) => {
            match load_config(config_path.as_str()).as_mut() {
                Ok(loaded_config) => {
                    match loaded_config.outbound_port_acks {
                        None => {
                            loaded_config.outbound_port_acks = Some(args.outbound_port_acks);
                        },
                        _ => {},
                    }
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
                outbound_port_data: args.outbound_port_data.expect("Argument `outbound_port_data` is required unless a config file is specified!"),
                outbound_port_acks: Some(args.outbound_port_acks),
                inbound_port: args.inbound_port.expect("Argument `inbound_port` is required unless a config file is specified!"),
                interval: Some(args.interval),
                inbound_poll_interval: Some(args.inbound_poll_interval),
            };
        }
    }

    println!("Starting input node for flow '{}' @ area '{}'", config.flow_name, config.area);

    let target: Arc<Mutex<SocketAddr>> = Arc::new(Mutex::new(SocketAddr::from(format!("{}:{}", config.target_ip, config.target_port).parse::<SocketAddr>().expect("No valid target address given. Use format: <ip>:<port>"))));

    let outbound_socket_data = UdpSocket::bind(format!("0.0.0.0:{}", config.outbound_port_data)).await.expect("Couldn't bind outbound socket");
    let outbound_socket_acks = UdpSocket::bind(format!("0.0.0.0:{}", config.outbound_port_acks.unwrap())).await.expect("Couldn't bind outbound socket");
    let inbound_socket = UdpSocket::bind(format!("0.0.0.0:{}", config.inbound_port)).await.expect("Couldn't bind inbound socket");
    // let timeout = Duration::from_millis(10);
    // inbound_socket.set_read_timeout(timeout.into()).expect("Couldn't set socket timeout");


    let mut buf = [0; 1024];

    let mut tasks: Vec<tokio::task::JoinHandle<()>> = vec![];

    let target_data_clone = target.clone();

    // send input data
    tasks.push(tokio::spawn(async move {

        let mut interval = time::interval(Duration::from_millis(config.interval.unwrap()));

        loop {

            interval.tick().await;

            let data = generate_input_data();

            let json = json!({
                "message": data.to_string(),
                "meta": {
                    "flow_name": config.flow_name,
                    "execution_area": config.area
                }
            });
            
            let target = target_data_clone.lock().unwrap().clone();
            println!("Sending data to {}: {}", target, data);
            outbound_socket_data.send_to(&json.to_string().as_bytes(), target).await.expect("Couldn't send data");

        }
    }));

    let target_updates_clone = target.clone();

    // receive target updates
    tasks.push(tokio::spawn(async move {

        loop {

            // check socket for incoming data
            if let Ok((message_length, src)) = inbound_socket.recv_from(&mut buf).await {
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
                        {
                            let mut target = target_updates_clone.lock().unwrap();
                            *target = new_target_address;
                        }
                        
                        // acknowledge
                        let json = json!({
                            "type": "updateTarget",
                            "success": true,
                        });
                        println!("Sending ACK to {}: {}", src, json.to_string());
                        // send 10 times to "make sure" it arrives
                        for _ in 0..10 {
                            outbound_socket_acks.send_to(&json.to_string().as_bytes(), src).await.expect("Couldn't send ACK");
                        }
                    }
                } 
    
            } else {
                // no data received
                // println!("No data received")
            }

        }

    }));

    future::join_all(tasks).await;

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
