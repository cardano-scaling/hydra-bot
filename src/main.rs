use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use argh::FromArgs;
use sha1::{Digest, Sha1};
use tracing::{error, info};

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
        drone: 1,
        max_players: 8,
        is_freedoom: 0,
        wad_sha1sum: wad_sha1.into(),
        deh_sha1sum: [0; 20],
        player_class: 0,
    };

    info!("Connecting with data: {:?}", connect_data);

    let mut retry_count = 0;
    const MAX_RETRIES: u32 = 3;

    while retry_count < MAX_RETRIES {
        match client.connect(server_addr, connect_data) {
            Ok(_) => {
                info!("Connected to server successfully");
                break;
            }
            Err(e) => {
                error!("Failed to connect to server: {}", e);
                if let Some(reject_reason) = client.get_reject_reason() {
                    error!("Server rejection reason: {}", reject_reason);
                }
                retry_count += 1;
                if retry_count < MAX_RETRIES {
                    info!("Retrying connection ({}/{})", retry_count, MAX_RETRIES);
                    std::thread::sleep(std::time::Duration::from_secs(5));
                } else {
                    return Err(e.into());
                }
            }
        }
    }

    info!("Waiting for game to start...");
    loop {
        client.run();
        if let Some(settings) = client.get_settings() {
            info!("Game started with settings: {:?}", settings);
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    info!("Game started, entering main game loop");
    game.start_loop();

    loop {
        game.tick(&mut client);
        client.run();

        if !client.is_connected() {
            info!("Disconnected from server");
            break;
        }

        thread::sleep(Duration::from_millis(1000 / 35)); // Aim for ~35 FPS
    }

    info!("Game loop ended");
    client.disconnect();

    Ok(())
}
