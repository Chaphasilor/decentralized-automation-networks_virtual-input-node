# Decentralized Automation Networks - Virtual Input Note

This is a Rust-based program that emulates an input node of an automation network. It will send out a message to a specific target every `<interval>` milliseconds.  
The input node is tied to a specific automation/flow, by which the incoming message will be processed.  
The input node accepts control commands on a specific UDP port. Right now this is used to specify a new target for the node (needed to transfer an automation), and to perform a UDP ping.

## Usage

All options can either be specified as CLI args, or inside a yaml-based config file (take a look at [the provided sample configuration](data/input_node1.config.yaml)).  
Any option that has a default value can be omitted from the config file to allow setting it using a CLI arg during startup.

```sh-session
$ cargo run -- --help  
A simple application emulating a physical input node

Usage: decentralized-automation-networks_virtual-input-node.exe [OPTIONS]

Options:
  -a, --area <AREA>
          area name
  -f, --flow <FLOW>
          flow name
  -t, --target-ip <TARGET_IP>
          The initial target ip
  -p, --target-port <TARGET_PORT>
          The initial target port
  -o, --outbound-port-data <OUTBOUND_PORT_DATA>
          The outgoing port for sending data
      --outbound-port-acks <OUTBOUND_PORT_ACKS>
          The outgoing port for sending ACKs [default: 0]
  -i, --inbound-port <INBOUND_PORT>
          The incoming port
      --interval <INTERVAL>
          data interval (ms) [default: 1000]
      --inbound-poll-interval <INBOUND_POLL_INTERVAL>
          inbound poll interval (ms) [default: 10]
  -c, --config <CONFIG>
          config file
  -h, --help
          Print help
  -V, --version
          Print version
```
