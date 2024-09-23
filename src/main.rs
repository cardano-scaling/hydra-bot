#![allow(unused)]

mod d_loop;
mod net_client;
mod net_packet;
mod net_structs;

use tracing::info;

use self::net_client::NetClient;

fn main() {
    tracing_subscriber::fmt::init();

    info!("Initializing client");
    let mut client = NetClient::new("Player1".to_string(), false);
    client.init();

    info!("Client initialized, starting main loop");

    d_loop::init();
    d_loop::d_start_game_loop();

    loop {
        d_loop::try_run_tics(&mut client);

        // Add some delay to prevent busy-waiting
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
