use serde::{Deserialize, Serialize};
use std::time::Instant;

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

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct ConnectData {
    pub gamemode: i32,
    pub gamemission: i32,
    pub lowres_turn: i32,
    pub drone: i32,
    pub max_players: i32,
    pub is_freedoom: i32,
    pub wad_sha1sum: [u8; 20],
    pub deh_sha1sum: [u8; 20],
    pub player_class: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct GameSettings {
    pub ticdup: i32,
    pub extratics: i32,
    pub deathmatch: i32,
    pub episode: i32,
    pub nomonsters: i32,
    pub fast_monsters: i32,
    pub respawn_monsters: i32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketType {
    Syn,
    Ack,
    Rejected,
    KeepAlive,
    WaitingData,
    GameStart,
    GameData,
    GameDataAck,
    Disconnect,
    DisconnectAck,
    ReliableAck,
    GameDataResend,
    ConsoleMessage,
    Query,
    QueryResponse,
    Launch,
    NatHolePunch,
}

impl PacketType {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(PacketType::Syn),
            1 => Some(PacketType::Ack),
            2 => Some(PacketType::Rejected),
            3 => Some(PacketType::KeepAlive),
            4 => Some(PacketType::WaitingData),
            5 => Some(PacketType::GameStart),
            6 => Some(PacketType::GameData),
            7 => Some(PacketType::GameDataAck),
            8 => Some(PacketType::Disconnect),
            9 => Some(PacketType::DisconnectAck),
            10 => Some(PacketType::ReliableAck),
            11 => Some(PacketType::GameDataResend),
            12 => Some(PacketType::ConsoleMessage),
            13 => Some(PacketType::Query),
            14 => Some(PacketType::QueryResponse),
            15 => Some(PacketType::Launch),
            16 => Some(PacketType::NatHolePunch),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            PacketType::Syn => 0,
            PacketType::Ack => 1,
            PacketType::Rejected => 2,
            PacketType::KeepAlive => 3,
            PacketType::WaitingData => 4,
            PacketType::GameStart => 5,
            PacketType::GameData => 6,
            PacketType::GameDataAck => 7,
            PacketType::Disconnect => 8,
            PacketType::DisconnectAck => 9,
            PacketType::ReliableAck => 10,
            PacketType::GameDataResend => 11,
            PacketType::ConsoleMessage => 12,
            PacketType::Query => 13,
            PacketType::QueryResponse => 14,
            PacketType::Launch => 15,
            PacketType::NatHolePunch => 16,
        }
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
