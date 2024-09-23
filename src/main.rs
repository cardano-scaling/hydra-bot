#![allow(unused)]

mod game;
mod net_client;
mod net_packet;
mod net_structs;

use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use self::game::Game;
use self::net_client::NetClient;
use self::net_structs::{ConnectData, NetAddr};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Initializing client");
    let mut client = NetClient::new("Player1".to_string(), false);
    client.init();

    info!("Initializing game");
    let mut game = Game::new();

    info!("Connecting to server");
    let server_addr = "97.94.129.234:2342".parse::<SocketAddr>()?;
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

    match client.connect(NetAddr::from(server_addr), connect_data) {
        Ok(_) => {
            info!("Connected to server successfully");
        }
        Err(e) => {
            error!("Failed to connect to server: {}", e);
            // Try to get more information about the connection failure
            if let Some(reject_reason) = client.get_reject_reason() {
                error!("Server rejection reason: {}", reject_reason);
            }
            return Err(e.into());
        }
    }

    if !client.is_connected() {
        error!("Connection process completed, but client is not connected");
        return Err("Failed to establish connection".into());
    }

    info!("Client and game initialized, starting main loop");

    game.d_start_game_loop();

    loop {
        game.try_run_tics(&mut client);

        // Run the client
        client.run();

        // Add some delay to prevent busy-waiting
        std::thread::sleep(Duration::from_millis(10));
        debug!("Completed a game loop iteration");

        // Check if the client is still connected
        if !client.is_connected() {
            error!("Lost connection to server");
            break;
        }
    }

    info!("Game loop ended");
    Ok(())
}
