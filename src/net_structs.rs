use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::time::Instant;

use crate::net_packet::NetPacket;

pub const MAXNETNODES: usize = 16;
pub const NET_MAXPLAYERS: usize = 8;
pub const MAXPLAYERNAME: usize = 30;
pub const BACKUPTICS: usize = 128;
pub const NET_MAGIC_NUMBER: u32 = 1454104972;
pub const NET_OLD_MAGIC_NUMBER: u32 = 3436803284;
pub const NET_RELIABLE_PACKET: u16 = 1 << 15;

// TicDiff Flags
pub const NET_TICDIFF_FORWARD: u32 = 1 << 0;
pub const NET_TICDIFF_SIDE: u32 = 1 << 1;
pub const NET_TICDIFF_TURN: u32 = 1 << 2;
pub const NET_TICDIFF_BUTTONS: u32 = 1 << 3;
pub const NET_TICDIFF_CONSISTANCY: u32 = 1 << 4;
pub const NET_TICDIFF_CHATCHAR: u32 = 1 << 5;
pub const NET_TICDIFF_RAVEN: u32 = 1 << 6;
pub const NET_TICDIFF_STRIFE: u32 = 1 << 7;

pub const BT_SPECIAL: u8 = 0x80;

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

impl NetProtocol {
    pub const NUM_PROTOCOLS: usize = 2;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetMasterPacketType {
    Add,
    AddResponse,
    Query,
    QueryResponse,
    GetMetadata,
    GetMetadataResponse,
    SignStart,
    SignStartResponse,
    SignEnd,
    SignEndResponse,
    NatHolePunch,
    NatHolePunchAll,
}

pub struct NetModule {
    pub init_client: fn() -> bool,
    pub init_server: fn() -> bool,
    pub send_packet: fn(addr: &NetAddr, packet: &NetPacket),
    pub recv_packet: fn(addr: &mut Option<NetAddr>, packet: &mut Option<NetPacket>) -> bool,
    pub addr_to_string: fn(addr: &NetAddr, buffer: &mut String, buffer_len: usize),
    pub free_address: fn(addr: &mut NetAddr),
    pub resolve_address: fn(addr: &str) -> Option<NetAddr>,
}

use std::net::SocketAddr;

#[derive(Debug, Clone, PartialEq)]
pub struct NetAddr {
    pub module: *mut NetModule,
    pub refcount: i32,
    pub handle: *mut c_void,
    pub socket_addr: SocketAddr,
}

impl From<SocketAddr> for NetAddr {
    fn from(addr: SocketAddr) -> Self {
        NetAddr {
            module: std::ptr::null_mut(),
            refcount: 1,
            handle: std::ptr::null_mut(),
            socket_addr: addr,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetContext {
    // Define fields as necessary
}

impl NetContext {
    pub fn recv_packet(&self) -> Option<(NetAddr, NetPacket)> {
        // Implement the logic to receive a packet
        None // Placeholder
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
pub struct NetQueryData {
    pub version: String,
    pub server_state: i32,
    pub num_players: i32,
    pub max_players: i32,
    pub gamemode: i32,
    pub gamemission: i32,
    pub description: String,
    pub protocol: NetProtocol,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
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
pub enum GameMode {
    Shareware,
    Registered,
    Commercial,
    Retail,
    Indetermined,
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
    WaitingLaunch,
    WaitingStart,
    InGame,
    DisconnectedSleep,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NetConnection {
    // Add necessary fields here
    pub state: ConnectionState,
    pub protocol: NetProtocol,
    pub connected: bool,
}

impl NetConnection {
    pub fn run(&mut self) {
        // Implement run logic
    }

    pub fn disconnect(&mut self) {
        // Implement disconnect logic
    }

    pub fn send_packet(&self, packet: &NetPacket, addr: &NetAddr) {
        // Implement send_packet logic
    }

    pub fn send_reliable_packet(&self, packet: &NetPacket) {
        // Implement send_reliable_packet logic
    }

    pub fn init_client(&mut self, addr: &NetAddr, connect_data: &ConnectData) {
        // Implement init_client logic
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    #[default]
    Connecting,
    Connected,
    Disconnected,
    DisconnectedSleep,
}

#[derive(Debug, Clone)]
pub struct SendQueueEntry {
    pub active: bool,
    pub seq: u32,
    pub time: Instant,
    pub cmd: NetTicDiff,
}
