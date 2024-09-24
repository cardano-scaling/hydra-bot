#![allow(unused)]

use argh::FromArgs;
use sha1::{Sha1, Digest};
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, error, info, warn};

mod game;
mod net_client;
mod net_packet;
mod net_structs;

use self::game::Game;
use self::net_client::NetClient;
use self::net_structs::{ConnectData, NetAddr};

#[derive(FromArgs)]
/// Doom client application
struct ClientArgs {
    /// server address to connect to
    #[argh(positional)]
    server_addr: String,

    /// path to the WAD file to load
    #[argh(positional)]
    wad_path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: ClientArgs = argh::from_env();

    info!("Initializing client");
    let mut client = match NetClient::new("Player1".to_string(), false) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to initialize client: {}", e);
            return Err(e.into());
        }
    };
    client.init();

    info!("Initializing game");
    let mut game = Game::new();

    info!("Connecting to server");
    let server_addr = args.server_addr.parse::<SocketAddr>()?;
    
    // Read WAD file and compute SHA1
    let mut wad_file = File::open(&args.wad_path)?;
    let mut wad_contents = Vec::new();
    wad_file.read_to_end(&mut wad_contents)?;
    let wad_sha1 = Sha1::digest(&wad_contents);

    let connect_data = ConnectData {
        gamemode: 0,    // Commercial
        gamemission: 2, // Doom 2
        lowres_turn: 0,
        drone: 0,
        max_players: 4,
        is_freedoom: 0,
        wad_sha1sum: wad_sha1.into(),
        deh_sha1sum: [0; 20], // Replace with actual SHA1 sum if DEH file is used
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

    game.start_loop();

    loop {
        game.tick(&mut client);

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
