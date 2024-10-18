use serde::{Deserialize, Serialize};
use std::time::Instant;

pub const NET_MAGIC_NUMBER: u32 = 0x56abe18c;
pub const NET_MAXPLAYERS: usize = 8;
pub const MAXPLAYERNAME: usize = 30;
pub const BACKUPTICS: usize = 128;

pub const NET_TICDIFF_FORWARD: u32 = 1 << 0;
pub const NET_TICDIFF_SIDE: u32 = 1 << 1;
pub const NET_TICDIFF_TURN: u32 = 1 << 2;
pub const NET_TICDIFF_BUTTONS: u32 = 1 << 3;
pub const NET_TICDIFF_CONSISTANCY: u32 = 1 << 4;
pub const NET_TICDIFF_CHATCHAR: u32 = 1 << 5;
pub const NET_TICDIFF_RAVEN: u32 = 1 << 6;
pub const NET_TICDIFF_STRIFE: u32 = 1 << 7;

pub mod client;
pub mod packet;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct TicCmd {
    pub forwardmove: i8,
    pub sidemove: i8,
    pub angleturn: i16,
    pub chatchar: u8,
    pub buttons: u8,
    pub consistancy: u8,
    pub buttons2: u8,
    pub inventory: i32,
    pub lookfly: u8,
    pub arti: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConnectData {
    pub gamemode: u8,
    pub gamemission: u8,
    pub lowres_turn: u8,
    pub drone: u8,
    pub max_players: u8,
    pub is_freedoom: u8,
    pub wad_sha1sum: [u8; 20],
    pub deh_sha1sum: [u8; 20],
    pub player_class: u8,
}

impl Default for ConnectData {
    fn default() -> Self {
        ConnectData {
            gamemode: 3,          // Correct gamemode (commercial)
            gamemission: 2,       // Correct gamemission (doom2)
            lowres_turn: 0,       // Should be 0 or 1
            drone: 0,             // 0 for regular player
            max_players: 4,       // Valid range is typically 1 to 4 or 1 to 8
            is_freedoom: 0,       // 0 if not using Freedoom
            wad_sha1sum: [0; 20], // Ensure correct SHA1 sum is used
            deh_sha1sum: [0; 20],
            player_class: 0, // 0 unless the game supports classes
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WaitData {
    pub num_players: i32,
    pub num_drones: i32,
    pub ready_players: i32,
    pub max_players: i32,
    pub is_controller: i32,
    pub consoleplayer: i32,
    pub player_names: [[char; MAXPLAYERNAME]; NET_MAXPLAYERS],
    pub player_addrs: [[char; MAXPLAYERNAME]; NET_MAXPLAYERS],
    pub wad_sha1sum: [u8; 20],
    pub deh_sha1sum: [u8; 20],
    pub is_freedoom: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct GameSettings {
    pub ticdup: i32,
    pub extratics: i32,
    pub deathmatch: i32,
    pub nomonsters: i32,
    pub fast_monsters: i32,
    pub respawn_monsters: i32,
    pub episode: i32,
    pub map: i32,
    pub skill: i32,
    pub gameversion: i32,
    pub lowres_turn: i32,
    pub new_sync: i32,
    pub timelimit: u32,
    pub loadgame: i32,
    pub random: i32,
    pub num_players: i32,
    pub consoleplayer: i32,
    pub player_classes: [i32; NET_MAXPLAYERS],
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    ChocolateDoom0,
    Unknown,
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketType {
    Syn = 0,
    Ack = 1,
    Rejected = 2,
    KeepAlive = 3,
    WaitingData = 4,
    GameStart = 5,
    GameData = 6,
    GameDataAck = 7,
    Disconnect = 8,
    DisconnectAck = 9,
    ReliableAck = 10,
    GameDataResend = 11,
    ConsoleMessage = 12,
    Query = 13,
    QueryResponse = 14,
    Launch = 15,
    NatHolePunch = 16,
}

impl PacketType {
    pub fn from_u16(value: u16) -> Option<Self> {
        use std::mem::transmute;
        if value <= PacketType::NatHolePunch as u16 {
            Some(unsafe { transmute::<u16, PacketType>(value) })
        } else {
            None
        }
    }

    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct TicDiff {
    pub diff: u32,
    pub cmd: TicCmd,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct FullTicCmd {
    pub latency: i32,
    pub seq: u32,
    pub playeringame: [bool; NET_MAXPLAYERS],
    pub cmds: [TicDiff; NET_MAXPLAYERS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    Shareware,
    Registered,
    Commercial,
    Retail,
    Indetermined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMission {
    Doom,
    Doom2,
    PackTnt,
    PackPlut,
    PackChex,
    PackHacx,
    Heretic,
    Hexen,
    Strife,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameVersion {
    Doom1_2,
    Doom1_666,
    Doom1_7,
    Doom1_8,
    Doom1_9,
    Hacx,
    Ultimate,
    Final,
    Final2,
    Chex,
    Heretic1_3,
    Hexen1_1,
    Strife1_2,
    Strife1_31,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameVariant {
    Vanilla,
    Freedoom,
    Freedm,
    BfgEdition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Skill {
    NoItems = -1,
    Baby = 0,
    Easy,
    Medium,
    Hard,
    Nightmare,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    WaitingLaunch,
    WaitingStart,
    InGame,
    Disconnecting,
}

#[derive(Clone, Copy)]
pub struct ServerRecv {
    pub active: bool,
    pub resend_time: Instant,
    pub cmd: FullTicCmd,
}

impl Default for ServerRecv {
    fn default() -> Self {
        Self {
            active: false,
            resend_time: Instant::now(),
            cmd: Default::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct ServerSend {
    pub active: bool,
    pub seq: u32,
    pub time: Instant,
    pub cmd: TicDiff,
}

impl Default for ServerSend {
    fn default() -> Self {
        Self {
            active: false,
            seq: 0,
            time: Instant::now(),
            cmd: Default::default(),
        }
    }
}
