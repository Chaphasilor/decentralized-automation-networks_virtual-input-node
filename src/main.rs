use clap::Parser;
use futures::future;
use serde_json::json;
use std::sync::Mutex;
use std::{error::Error, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, time};

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
    #[arg(short = 't', long)]
    target_ip: Option<String>,
    /// The initial target port
    #[arg(short = 'p', long)]
    target_port: Option<u16>,
    /// The outgoing port for sending data
    #[arg(short, long)]
    outbound_port_data: Option<u16>,
    /// The outgoing port for sending ACKs
    #[arg(long, default_value = "0")]
    outbound_port_acks: u16,
    /// The incoming port
    #[arg(short = 'i', long)]
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
async fn main() {
    let args = Args::parse();
    let config: Config;

    // load config form file and apply overwrites if given
    match args.config {
        Some(config_path) => match load_config(config_path.as_str()).as_mut() {
            Ok(loaded_config) => {
                if loaded_config.outbound_port_acks.is_none() {
                    loaded_config.outbound_port_acks = Some(args.outbound_port_acks);
                }
                if loaded_config.interval.is_none() {
                    loaded_config.interval = Some(args.interval);
                }
                if loaded_config.inbound_poll_interval.is_none() {
                    loaded_config.inbound_poll_interval = Some(args.inbound_poll_interval);
                }

                config = loaded_config.clone();
                println!("Config loaded: {:?}", config);
            }
            Err(e) => {
                panic!("Couldn't load config: {}", e);
            }
        },
        None => {
            // create config from args
            config = Config {
                area: args
                    .area
                    .expect("Argument `area` is required unless a config file is specified!"),
                flow_name: args
                    .flow
                    .expect("Argument `flow` is required unless a config file is specified!"),
                target_ip: args
                    .target_ip
                    .expect("Argument `target_ip` is required unless a config file is specified!"),
                target_port: args.target_port.expect(
                    "Argument `target_port` is required unless a config file is specified!",
                ),
                outbound_port_data: args.outbound_port_data.expect(
                    "Argument `outbound_port_data` is required unless a config file is specified!",
                ),
                outbound_port_acks: Some(args.outbound_port_acks),
                inbound_port: args.inbound_port.expect(
                    "Argument `inbound_port` is required unless a config file is specified!",
                ),
                interval: Some(args.interval),
                inbound_poll_interval: Some(args.inbound_poll_interval),
            };
        }
    }

    println!(
        "Starting input node for flow '{}' @ area '{}'",
        config.flow_name, config.area
    );

    let target: Arc<Mutex<SocketAddr>> = Arc::new(Mutex::new(
        format!("{}:{}", config.target_ip, config.target_port)
            .parse::<SocketAddr>()
            .expect("No valid target address given. Use format: <ip>:<port>"),
    ));

    let outbound_socket_data = UdpSocket::bind(format!("0.0.0.0:{}", config.outbound_port_data))
        .await
        .expect("Couldn't bind outbound socket");
    let outbound_socket_acks =
        UdpSocket::bind(format!("0.0.0.0:{}", config.outbound_port_acks.unwrap()))
            .await
            .expect("Couldn't bind outbound socket");
    let inbound_socket = UdpSocket::bind(format!("0.0.0.0:{}", config.inbound_port))
        .await
        .expect("Couldn't bind inbound socket");

    let mut buf = [0; 1024];

    let mut tasks: Vec<tokio::task::JoinHandle<()>> = vec![];

    let target_data_clone = target.clone();

    // generate and send input data
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

            let target = *target_data_clone.lock().unwrap();
            println!("Sending data to {}: {}", target, data);
            outbound_socket_data
                .send_to(json.to_string().as_bytes(), target)
                .await
                .expect("Couldn't send data");
        }
    }));

    let target_updates_clone = target.clone();

    // receive target updates
    tasks.push(tokio::spawn(async move {
        loop {
            // check socket for incoming data
            if let Ok((message_length, src)) = inbound_socket.recv_from(&mut buf).await {
                // convert to string
                let message = String::from_utf8(buf[..message_length].into())
                    .expect("Couldn't convert to String");
                println!("Received data from {}: {}", src, message);

                // parse json
                let json: serde_json::Value =
                    serde_json::from_str(&message).expect("Couldn't parse JSON");
                if let Some(message_type) = json["type"].as_str() {
                    match message_type {
                        "updateTarget" => {
                            // take 10k part from the new target port and fill the rest with the old one
                            let new_target_port_base = json["target_port_base"]
                                .as_u64()
                                .expect("No target base port given")
                                as u16;
                            let new_target_port =
                                new_target_port_base + (config.target_port % 10000);
                            println!("New target port: {}", new_target_port);

                            let new_target_address_string = format!(
                                "{}:{}",
                                json["target"].as_str().expect("No target ip given"),
                                new_target_port
                            );
                            let new_target_address = new_target_address_string
                                .parse::<SocketAddr>()
                                .unwrap_or_else(|_| {
                                    panic!(
                                        "Target not updated because target address was invalid: {}",
                                        new_target_address_string
                                    )
                                });
                            {
                                let mut target = target_updates_clone.lock().unwrap();
                                *target = new_target_address;
                            }

                            // acknowledge
                            let json = json!({
                                "type": "updateTarget",
                                "success": true,
                            });
                            println!("Sending ACK to {}: {}", src, json);
                            // send 10 times to "make sure" it arrives
                            for _ in 0..10 {
                                outbound_socket_acks
                                    .send_to(json.to_string().as_bytes(), src)
                                    .await
                                    .expect("Couldn't send ACK");
                            }
                        }
                        "udpPing" => {
                            let start = std::time::SystemTime::now();
                            let time = start
                                .duration_since(std::time::UNIX_EPOCH)
                                .expect("Couldn't get system time");
                            let return_buf = (time.as_micros() as u64).to_be_bytes();
                            let return_address = json["replyTo"]
                                .as_str()
                                .unwrap()
                                .parse::<SocketAddr>()
                                .expect("No return address given");
                            // send current system time back to sender
                            outbound_socket_acks
                                .send_to(&return_buf, &return_address)
                                .await
                                .unwrap();
                            println!("Sent UDP ping response to {}", return_address);
                        }
                        _ => {}
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
    rand::random::<u16>()
}

fn load_config(path: &str) -> Result<Config, Box<dyn Error>> {
    let config = std::fs::read_to_string(path)?;

    let config = serde_yaml::from_str::<Config>(&config);

    println!("config: {:?}", config);

    config.map_err(|err| err.into())
}
