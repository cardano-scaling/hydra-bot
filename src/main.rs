#![allow(unused)]

mod game;
mod net_client;
mod net_packet;
mod net_structs;

use tracing::{info, debug, error};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

use self::net_client::NetClient;
use self::game::Game;
use self::net_structs::{ConnectData, NetAddr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Initializing client");
    let mut client = NetClient::new("Player1".to_string(), false);
    client.init();

    info!("Initializing game");
    let mut game = Game::new();

    info!("Connecting to server");
    let server_addr = "127.0.0.1:2342".parse::<SocketAddr>()?;
    let connect_data = ConnectData {
        gamemode: 0,
        gamemission: 0,
        lowres_turn: 0,
        drone: 0,
        max_players: 4,
        is_freedoom: 0,
        wad_sha1sum: [0; 20],
        deh_sha1sum: [0; 20],
        player_class: 0,
    };

    match client.connect(NetAddr::from(server_addr), connect_data).await {
        Ok(_) => info!("Connected to server successfully"),
        Err(e) => {
            error!("Failed to connect to server: {}", e);
            return Err(e.into());
        }
    }

    info!("Client and game initialized, starting main loop");

    game.d_start_game_loop();

    loop {
        game.try_run_tics(&mut client);

        // Add some delay to prevent busy-waiting
        sleep(Duration::from_millis(10)).await;
        debug!("Completed a game loop iteration");
    }
}
