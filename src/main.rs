use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use argh::FromArgs;
use sha1::{Digest, Sha1};
use tracing::{debug, error, info};

mod game;
mod net;

use self::game::Game;
use self::net::client::Client;
use self::net::{ConnectData, GameMission, GameMode};

#[derive(FromArgs)]
/// An AI player implementation compatible with Chocolate Doom v3.
struct Args {
    /// which server to connect to
    #[argh(option, short = 'a')]
    address: String,

    /// the WAD path to load
    #[argh(option, short = 'i')]
    iwad: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: Args = argh::from_env();

    info!("Initializing client");
    let mut client = Client::new("HydraBot".to_string(), true)?;
    client.init();

    info!("Initializing game");
    let mut game = Game::new();

    info!("Connecting to server");
    let server_addr = args.address.parse::<SocketAddr>()?;

    // Read WAD file and compute SHA1
    let mut wad_file = File::open(&args.iwad)?;
    let mut wad_contents = Vec::new();
    wad_file.read_to_end(&mut wad_contents)?;
    let wad_sha1 = Sha1::digest(&wad_contents);

    let connect_data = ConnectData {
        gamemode: GameMode::Commercial as i32,
        gamemission: GameMission::Doom2 as i32,
        lowres_turn: 0,
        drone: 1, // Set to 1 for bot
        max_players: 4,
        is_freedoom: 0,
        wad_sha1sum: wad_sha1.into(),
        deh_sha1sum: [0; 20],
        player_class: 0,
    };

    match client.connect(server_addr, connect_data) {
        Ok(_) => {
            info!("Connected to server successfully");
        }
        Err(e) => {
            error!("Failed to connect to server: {}", e);
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
        client.run();
        thread::sleep(Duration::from_millis(10));
        debug!("Tick");

        if !client.is_connected() {
            error!("Lost connection to server");
            break;
        }
    }

    info!("Game loop ended");

    Ok(())
}
