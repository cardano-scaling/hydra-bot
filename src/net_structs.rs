use serde::{Deserialize, Serialize};
use std::time::Instant;

pub const MAXNETNODES: usize = 16;
pub const NET_MAXPLAYERS: usize = 8;
pub const MAXPLAYERNAME: usize = 30;
pub const BACKUPTICS: usize = 128;
pub const NET_RELIABLE_PACKET: u16 = 1 << 15;

pub const NET_TICDIFF_FORWARD: u32 = 1 << 0;
pub const NET_TICDIFF_SIDE: u32 = 1 << 1;
pub const NET_TICDIFF_TURN: u32 = 1 << 2;
pub const NET_TICDIFF_BUTTONS: u32 = 1 << 3;
pub const NET_TICDIFF_CONSISTANCY: u32 = 1 << 4;
pub const NET_TICDIFF_CHATCHAR: u32 = 1 << 5;
pub const NET_TICDIFF_RAVEN: u32 = 1 << 6;
pub const NET_TICDIFF_STRIFE: u32 = 1 << 7;

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
pub enum NetProtocol {
    #[default]
    ChocolateDoom0,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetPacketType {
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

impl NetPacketType {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(NetPacketType::Syn),
            1 => Some(NetPacketType::Ack),
            2 => Some(NetPacketType::Rejected),
            3 => Some(NetPacketType::KeepAlive),
            4 => Some(NetPacketType::WaitingData),
            5 => Some(NetPacketType::GameStart),
            6 => Some(NetPacketType::GameData),
            7 => Some(NetPacketType::GameDataAck),
            8 => Some(NetPacketType::Disconnect),
            9 => Some(NetPacketType::DisconnectAck),
            10 => Some(NetPacketType::ReliableAck),
            11 => Some(NetPacketType::GameDataResend),
            12 => Some(NetPacketType::ConsoleMessage),
            13 => Some(NetPacketType::Query),
            14 => Some(NetPacketType::QueryResponse),
            15 => Some(NetPacketType::Launch),
            16 => Some(NetPacketType::NatHolePunch),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            NetPacketType::Syn => 0,
            NetPacketType::Ack => 1,
            NetPacketType::Rejected => 2,
            NetPacketType::KeepAlive => 3,
            NetPacketType::WaitingData => 4,
            NetPacketType::GameStart => 5,
            NetPacketType::GameData => 6,
            NetPacketType::GameDataAck => 7,
            NetPacketType::Disconnect => 8,
            NetPacketType::DisconnectAck => 9,
            NetPacketType::ReliableAck => 10,
            NetPacketType::GameDataResend => 11,
            NetPacketType::ConsoleMessage => 12,
            NetPacketType::Query => 13,
            NetPacketType::QueryResponse => 14,
            NetPacketType::Launch => 15,
            NetPacketType::NatHolePunch => 16,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct NetTicDiff {
    pub diff: u32,
    pub cmd: TicCmd,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct NetFullTicCmd {
    pub latency: i32,
    pub seq: u32,
    pub playeringame: [bool; NET_MAXPLAYERS],
    pub cmds: [NetTicDiff; NET_MAXPLAYERS],
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NetWaitData {
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
pub struct NetServerRecv {
    pub active: bool,
    pub resend_time: Instant,
    pub cmd: NetFullTicCmd,
}

impl Default for NetServerRecv {
    fn default() -> Self {
        Self {
            active: false,
            resend_time: Instant::now(),
            cmd: Default::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct NetServerSend {
    pub active: bool,
    pub seq: u32,
    pub time: Instant,
    pub cmd: NetTicDiff,
}

impl Default for NetServerSend {
    fn default() -> Self {
        Self {
            active: false,
            seq: 0,
            time: Instant::now(),
            cmd: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetReliablePacket {
    pub packet: crate::net_packet::NetPacket,
    pub seq: u8,
    pub last_send_time: Instant,
}
