#![allow(unused)]

mod game;
mod net_client;
mod net_packet;
mod net_structs;

use tracing::info;

use self::net_client::NetClient;
use self::game::Game;

fn main() {
    tracing_subscriber::fmt::init();

    info!("Initializing client");
    let mut client = NetClient::new("Player1".to_string(), false);
    client.init();

    info!("Initializing game");
    let mut game = Game::new();

    info!("Client and game initialized, starting main loop");

    game.d_start_game_loop();

    loop {
        game.try_run_tics(&mut client);

        // Add some delay to prevent busy-waiting
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
