use litecode_protocol::PROTOCOL_VERSION;
use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("status") => {
            print_status();
            ExitCode::SUCCESS
        }
        Some("version" | "--version" | "-V") => {
            println!("litecode-agent {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("help" | "--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("unknown command: {command}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn print_status() {
    println!("LiteCode agent");
    println!("version: {}", env!("CARGO_PKG_VERSION"));
    println!("protocol: {PROTOCOL_VERSION}");
    println!("platform: {}", std::env::consts::OS);
    println!("state: foundation");
}

fn print_help() {
    println!("Usage: litecode-agent [status|version|help]");
}
