use rand::prelude::*;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{io, thread};
use tracing::{debug, error, info, warn};

use super::packet::Packet;
use super::{NET_RELIABLE_PACKET, *};

const NET_MAGIC_NUMBER: u32 = 1454104972;
const KEEPALIVE_PERIOD: Duration = Duration::from_secs(1);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RETRIES: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    WaitingLaunch,
    WaitingStart,
    InGame,
    Disconnecting,
}

pub struct Client {
    socket: UdpSocket,
    state: ClientState,
    server_addr: Option<SocketAddr>,
    settings: Option<GameSettings>,
    reject_reason: Option<String>,
    player_name: String,
    drone: bool,
    recv_window_start: u32,
    recv_window: [ServerRecv; BACKUPTICS],
    send_queue: [ServerSend; BACKUPTICS],
    need_acknowledge: bool,
    gamedata_recv_time: Instant,
    last_latency: i32,
    net_local_wad_sha1sum: [u8; 20],
    net_local_deh_sha1sum: [u8; 20],
    net_local_is_freedoom: bool,
    net_waiting_for_launch: bool,
    net_client_connected: bool,
    net_client_received_wait_data: bool,
    net_client_wait_data: WaitData,
    last_send_time: Instant,
    last_ticcmd: TicCmd,
    recvwindow_cmd_base: [TicCmd; NET_MAXPLAYERS],
    start_time: Instant,
    num_retries: u32,
    protocol: Protocol,
    gamemode: i32,
    gamemission: i32,
    lowres_turn: i32,
    max_players: i32,
    is_freedoom: i32,
    player_class: i32,
    reliable_packets: Vec<ReliablePacket>,
    reliable_send_seq: u8,
    reliable_recv_seq: u8,
    pid_controller: PIDController,
}

struct PIDController {
    kp: f32,
    ki: f32,
    kd: f32,
    cumul_error: i32,
    last_error: i32,
}

impl PIDController {
    fn new(kp: f32, ki: f32, kd: f32) -> Self {
        PIDController {
            kp,
            ki,
            kd,
            cumul_error: 0,
            last_error: 0,
        }
    }

    fn update(&mut self, error: i32) -> i32 {
        self.cumul_error += error;
        let d_error = error - self.last_error;
        self.last_error = error;

        (self.kp * error as f32 - self.ki * self.cumul_error as f32 + self.kd * d_error as f32)
            as i32
    }
}

impl Client {
    pub fn new(player_name: String, drone: bool) -> io::Result<Self> {
        info!(
            "Creating new Client: player_name={}, drone={}",
            player_name, drone
        );

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(Client {
            socket,
            state: ClientState::Disconnected,
            server_addr: None,
            settings: None,
            reject_reason: None,
            player_name,
            drone,
            recv_window_start: 0,
            recv_window: [ServerRecv::default(); BACKUPTICS],
            send_queue: [ServerSend::default(); BACKUPTICS],
            need_acknowledge: false,
            gamedata_recv_time: Instant::now(),
            last_latency: 0,
            net_local_wad_sha1sum: [0; 20],
            net_local_deh_sha1sum: [0; 20],
            net_local_is_freedoom: false,
            net_waiting_for_launch: false,
            net_client_connected: false,
            net_client_received_wait_data: false,
            net_client_wait_data: WaitData::default(),
            last_send_time: Instant::now(),
            last_ticcmd: TicCmd::default(),
            recvwindow_cmd_base: [TicCmd::default(); NET_MAXPLAYERS],
            num_retries: 0,
            start_time: Instant::now(),
            protocol: Protocol::ChocolateDoom0,
            gamemode: 0,
            gamemission: 0,
            lowres_turn: 0,
            max_players: 0,
            is_freedoom: 0,
            player_class: 0,
            reliable_packets: Vec::new(),
            reliable_send_seq: 0,
            reliable_recv_seq: 0,
            pid_controller: PIDController::new(0.1, 0.01, 0.02),
        })
    }

    pub fn get_reject_reason(&self) -> Option<&str> {
        self.reject_reason.as_deref()
    }

    pub fn init(&mut self) {
        debug!("Initializing Client");
        self.init_bot();
        self.net_client_connected = false;
        self.net_client_received_wait_data = false;
        self.net_waiting_for_launch = false;

        if self.player_name.is_empty() {
            self.player_name = Self::get_player_name();
        }
        debug!("Player name set to: {}", self.player_name);
    }

    fn init_bot(&mut self) {
        if self.drone {
            debug!("Initializing bot-specific settings");
        }
    }

    fn get_player_name() -> String {
        std::env::args()
            .nth(1)
            .or_else(|| std::env::var("USER").ok())
            .or_else(|| std::env::var("USERNAME").ok())
            .unwrap_or_else(Self::get_random_pet_name)
    }

    fn get_random_pet_name() -> String {
        let pet_names = ["Fluffy", "Buddy", "Max", "Charlie", "Lucy", "Bailey"];
        let mut rng = rand::thread_rng();
        pet_names.choose(&mut rng).unwrap_or(&"Player").to_string()
    }

    pub fn run(&mut self) {
        self.run_bot();
        self.receive_packets();
        self.handle_state();
        self.send_keepalive();
        self.check_resends();
    }

    fn receive_packets(&mut self) {
        let mut buf = [0u8; 4096];
        while let Ok((size, addr)) = self.socket.recv_from(&mut buf) {
            debug!("Received {} bytes from {:?}", size, addr);
            let packet_data = buf[..size].to_vec();
            let mut packet = Packet {
                data: packet_data,
                pos: 0,
            };
            self.parse_packet(&mut packet);
        }
    }

    fn handle_state(&mut self) {
        match self.state {
            ClientState::Connecting => self.handle_connecting(),
            ClientState::Connected | ClientState::WaitingLaunch => self.handle_waiting(),
            ClientState::InGame => self.handle_in_game(),
            ClientState::Disconnecting => self.handle_disconnecting(),
            _ => debug!("Current state: {:?}", self.state),
        }
    }

    fn handle_connecting(&mut self) {
        let elapsed = self.start_time.elapsed();
        debug!("Connecting... Time elapsed: {:?}", elapsed);
        if elapsed > CONNECTION_TIMEOUT {
            self.handle_connection_timeout();
        }
    }

    fn handle_waiting(&mut self) {
        self.net_waiting_for_launch = true;
        debug!("Waiting for launch");
    }

    fn handle_in_game(&mut self) {
        self.advance_window();
    }

    fn handle_disconnecting(&mut self) {
        if self.start_time.elapsed() > Duration::from_secs(5) {
            self.handle_disconnection_timeout();
        }
    }

    fn handle_connection_timeout(&mut self) {
        warn!("Connection attempt timed out");
        self.reject_reason = Some("Connection attempt timed out".to_string());
        self.state = ClientState::Disconnected;
        self.shutdown();
    }

    fn handle_disconnection_timeout(&mut self) {
        warn!("Disconnection timed out");
        self.state = ClientState::Disconnected;
        self.shutdown();
    }

    fn send_keepalive(&mut self) {
        if (self.state == ClientState::Connected || self.state == ClientState::InGame)
            && self.last_send_time.elapsed() > KEEPALIVE_PERIOD
        {
            let mut packet = Packet::new();
            packet.write_u16(PacketType::GameDataAck.to_u16());
            packet.write_u8((self.recv_window_start & 0xff) as u8);
            self.send_packet(&packet);
            self.last_send_time = Instant::now();
        }
    }

    fn shutdown(&mut self) {
        self.state = ClientState::Disconnected;
        self.net_client_connected = false;
    }

    fn parse_packet(&mut self, packet: &mut Packet) {
        let original_data = packet.data.clone();
        if let Some(packet_type) = packet.read_u16().and_then(PacketType::from_u16) {
            debug!(
                "Received packet: type={:?}, data={:x?}",
                packet_type, original_data
            );
            match packet_type {
                PacketType::Syn => self.parse_syn(packet),
                PacketType::Rejected => self.parse_reject(packet),
                PacketType::WaitingData => self.parse_waiting_data(packet),
                PacketType::Launch => self.parse_launch(packet),
                PacketType::GameStart => self.parse_game_start(packet),
                PacketType::GameData => self.parse_game_data(packet),
                PacketType::GameDataResend => self.parse_resend_request(packet),
                PacketType::ConsoleMessage => self.parse_console_message(packet),
                PacketType::Disconnect => self.parse_disconnect(packet),
                PacketType::DisconnectAck => self.parse_disconnect_ack(packet),
                PacketType::KeepAlive => debug!("Received keep-alive packet"),
                _ => warn!("Unhandled packet type: {:?}", packet_type),
            }
        } else {
            warn!("Unknown packet type: {:x?}", original_data);
        }
    }

    fn parse_disconnect(&mut self, packet: &mut Packet) {
        info!("Received disconnect request from server");
        self.send_disconnect_ack(packet);
        self.state = ClientState::Disconnected;
        self.shutdown();
    }

    fn parse_disconnect_ack(&mut self, _packet: &mut Packet) {
        if self.state == ClientState::Disconnecting {
            info!("Received disconnect acknowledgement");
            self.state = ClientState::Disconnected;
            self.shutdown();
        }
    }

    fn parse_syn(&mut self, packet: &mut Packet) {
        debug!("Processing SYN response");
        let server_version = packet.read_safe_string().unwrap_or_default();
        debug!("Server version: {}", server_version);

        if let Some(protocol) = self.negotiate_protocol(packet) {
            self.protocol = protocol;
            info!("Connected to server");
            self.state = ClientState::Connected;

            // Send an ACK packet in response to the SYN
            self.send_ack(packet);

            if server_version != env!("CARGO_PKG_VERSION") {
                warn!(
                    "Version mismatch: Client is '{}', but the server is '{}'. \
                    This mismatch may cause the game to desynchronize.",
                    env!("CARGO_PKG_VERSION"),
                    server_version
                );
            }
        } else {
            error!("No common protocol");
            self.reject_reason = Some("No common protocol".to_string());
        }
    }

    fn send_ack(&mut self, packet: &mut Packet) {
        packet.write_u16(PacketType::Ack.to_u16());
        packet.write_protocol(self.protocol);
        self.send_packet(&packet);
        info!("ACK sent to server");
    }

    fn negotiate_protocol(&self, packet: &mut Packet) -> Option<Protocol> {
        let num_protocols = packet.read_u8().unwrap_or(0);
        for _ in 0..num_protocols {
            let protocol = packet.read_protocol();
            if protocol == Protocol::ChocolateDoom0 {
                return Some(protocol);
            }
        }
        None
    }

    fn parse_reject(&mut self, packet: &mut Packet) {
        if self.state == ClientState::Connecting {
            if let Some(msg) = packet.read_safe_string() {
                warn!("Connection rejected: {}", msg);
                self.state = ClientState::Disconnected;
                self.reject_reason = Some(msg);
                self.shutdown();
            }
        }
    }

    fn send_disconnect_ack(&self, packet: &mut Packet) {
        packet.write_u16(PacketType::DisconnectAck.to_u16());
        packet.write_u32(0x80);
        self.send_packet(&packet);
    }

    fn parse_waiting_data(&mut self, packet: &mut Packet) {
        if let Some(wait_data) = packet.read_wait_data() {
            if self.validate_wait_data(&wait_data) {
                self.net_client_wait_data = wait_data;
                self.net_client_received_wait_data = true;

                debug!("Received waiting data: {:?}", self.net_client_wait_data);

                self.max_players = self.net_client_wait_data.max_players;
                self.is_freedoom = self.net_client_wait_data.is_freedoom;

                // Send an ACK in response to waiting data
                self.send_ack(packet);
            }
        }
    }

    fn send_waiting_data_response(&mut self) {
        let mut packet = Packet::new();
        packet.write_u16(PacketType::WaitingData.to_u16());
        packet.write_string(&self.player_name);
        self.send_packet(&packet);
        info!("Waiting data response sent to server");
    }

    fn validate_wait_data(&self, wait_data: &WaitData) -> bool {
        wait_data.num_players <= wait_data.max_players
            && wait_data.ready_players <= wait_data.num_players
            && wait_data.max_players <= NET_MAXPLAYERS as i32
            && ((wait_data.consoleplayer >= 0 && !self.drone)
                || (wait_data.consoleplayer < 0 && self.drone)
                || ((wait_data.consoleplayer as usize) < wait_data.num_players as usize))
    }

    fn parse_launch(&mut self, packet: &mut Packet) {
        debug!("Processing launch packet");
        if self.state == ClientState::WaitingLaunch {
            if let Some(num_players) = packet.read_u8() {
                self.net_client_wait_data.num_players = num_players as i32;
                self.state = ClientState::WaitingStart;
                info!("Now waiting to start the game");

                // Send a response to confirm receipt of launch packet
                self.send_launch_response();
            }
        } else {
            warn!(
                "Received launch packet in incorrect state: {:?}",
                self.state
            );
        }
    }

    fn send_launch_response(&mut self) {
        let mut packet = Packet::new();
        packet.write_u16(PacketType::Launch.to_u16());
        self.send_packet(&packet);
        info!("Launch response sent to server");
    }

    fn parse_game_start(&mut self, packet: &mut Packet) {
        debug!("Processing game start packet");

        if let Some(settings) = packet.read_settings() {
            if self.validate_game_settings(&settings) {
                info!("Initiating game state with settings: {:?}", settings);
                self.state = ClientState::InGame;
                self.settings = Some(settings);
                self.init_game_state();

                self.lowres_turn = settings.lowres_turn;
                self.player_class = settings.player_classes[settings.consoleplayer as usize];

                // Send an ACK in response to game start
                self.send_ack(packet);
            }
        }
    }

    fn send_game_start_response(&mut self) {
        let mut packet = Packet::new();
        packet.write_u16(PacketType::GameStart.to_u16());
        self.send_packet(&packet);
        info!("Game start response sent to server");
    }

    fn validate_game_settings(&self, settings: &GameSettings) -> bool {
        settings.num_players <= NET_MAXPLAYERS as i32
            && (settings.consoleplayer as usize) < settings.num_players as usize
            && ((self.drone && settings.consoleplayer < 0)
                || (!self.drone && settings.consoleplayer >= 0))
    }

    fn init_game_state(&mut self) {
        self.recv_window_start = 0;
        self.recv_window = [ServerRecv::default(); BACKUPTICS];
        self.send_queue = [ServerSend::default(); BACKUPTICS];
    }

    fn parse_game_data(&mut self, packet: &mut Packet) {
        debug!("Processing game data packet");
        if let (Some(seq), Some(num_tics)) = (packet.read_u8(), packet.read_u8()) {
            let seq = self.expand_tic_num(seq as u32);
            debug!("Game data received, seq={}, num_tics={}", seq, num_tics);

            let lowres_turn = self.settings.as_ref().map_or(false, |s| s.lowres_turn != 0);

            for i in 0..num_tics {
                if let Some(cmd) = packet.read_full_ticcmd(lowres_turn) {
                    self.store_received_tic(seq + i as u32, cmd);
                }
            }

            self.need_acknowledge = true;
            self.gamedata_recv_time = Instant::now();
            self.check_for_missing_tics(seq);

            // Send an immediate ACK for the game data
            self.send_game_data_ack();
        }
    }

    fn store_received_tic(&mut self, seq: u32, cmd: FullTicCmd) {
        let index = (seq - self.recv_window_start) as usize;
        if index < BACKUPTICS {
            self.recv_window[index].active = true;
            self.recv_window[index].cmd = cmd;
            debug!("Stored tic {} in receive window", seq);
            self.update_clock_sync(seq, cmd.latency);
        }
    }

    fn check_for_missing_tics(&mut self, seq: u32) {
        let resend_end = seq as i32 - self.recv_window_start as i32;
        if resend_end > 0 {
            let mut resend_start = resend_end - 1;
            while resend_start >= 0 && !self.recv_window[resend_start as usize].active {
                resend_start -= 1;
            }
            if resend_start < resend_end - 1 {
                self.send_resend_request(
                    self.recv_window_start + resend_start as u32 + 1,
                    self.recv_window_start + resend_end as u32 - 1,
                );
            }
        }
    }

    fn parse_resend_request(&mut self, packet: &mut Packet) {
        debug!("Processing resend request");
        if self.drone {
            warn!("Error: Resend request but we are a drone");
            return;
        }

        if let (Some(start), Some(num_tics)) = (packet.read_i32(), packet.read_u8()) {
            let end = start + num_tics as i32 - 1;
            debug!("Resend request: start={}, num_tics={}", start, num_tics);

            let (resend_start, resend_end) = self.calculate_resend_range(start as u32, end as u32);

            if resend_start <= resend_end {
                debug!("Resending tics {}-{}", resend_start, resend_end);
                self.send_tics(resend_start, resend_end);
            } else {
                warn!("Don't have the tics to resend");
            }
        }
    }

    fn calculate_resend_range(&self, start: u32, end: u32) -> (u32, u32) {
        let mut resend_start = start;
        let mut resend_end = end;

        while resend_start <= resend_end {
            let index = resend_start as usize % BACKUPTICS;
            if self.send_queue[index].active && self.send_queue[index].seq == resend_start {
                break;
            }
            resend_start += 1;
        }

        while resend_start <= resend_end {
            let index = resend_end as usize % BACKUPTICS;
            if self.send_queue[index].active && self.send_queue[index].seq == resend_end {
                break;
            }
            resend_end -= 1;
        }

        (resend_start, resend_end)
    }

    fn parse_console_message(&self, packet: &mut Packet) {
        if let Some(msg) = packet.read_string() {
            info!("Message from server:\n{}", msg);
        }
    }

    fn expand_tic_num(&self, b: u32) -> u32 {
        let l = self.recv_window_start & 0xff;
        let h = self.recv_window_start & !0xff;
        let mut result = h | b;

        if l < 0x40 && b > 0xb0 {
            result = result.wrapping_sub(0x100);
        }
        if l > 0xb0 && b < 0x40 {
            result = result.wrapping_add(0x100);
        }

        result
    }

    fn update_clock_sync(&mut self, seq: u32, remote_latency: i32) {
        let latency = self.send_queue[seq as usize % BACKUPTICS]
            .time
            .elapsed()
            .as_millis() as i32;
        let error = latency - remote_latency;

        let offset_ms = self.pid_controller.update(error);

        self.last_latency = latency;

        debug!(
            "Latency {}, remote {}, offset={}ms",
            latency, remote_latency, offset_ms
        );
    }

    fn send_resend_request(&mut self, start: u32, end: u32) {
        let mut packet = Packet::new();
        packet.write_u16(PacketType::GameDataResend.to_u16());
        packet.write_i32(start as i32);
        packet.write_u8((end - start + 1) as u8);
        self.send_packet(&packet);

        let now = Instant::now();
        for i in start..=end {
            let index = (i - self.recv_window_start) as usize;
            if index < BACKUPTICS {
                self.recv_window[index].resend_time = now;
            }
        }
        debug!("Sent resend request for tics {}-{}", start, end);
    }

    fn send_game_data_ack(&mut self) {
        let mut packet = Packet::new();
        packet.write_u16(PacketType::GameDataAck.to_u16());
        packet.write_u8((self.recv_window_start & 0xff) as u8);
        self.send_packet(&packet);
        self.need_acknowledge = false;
        debug!("Game data acknowledgment sent");
    }

    fn send_tics(&mut self, start: u32, end: u32) {
        if !self.net_client_connected {
            return;
        }

        let mut packet = Packet::new();
        packet.write_u16(PacketType::GameData.to_u16());
        packet.write_u8((self.recv_window_start & 0xff) as u8);
        packet.write_u8((start & 0xff) as u8);
        packet.write_u8(((end - start + 1) & 0xff) as u8);

        let lowres_turn = self.settings.as_ref().map_or(false, |s| s.lowres_turn != 0);

        for tic in start..=end {
            if let Some(send_obj) = self.send_queue.get(tic as usize % BACKUPTICS) {
                packet.write_i16(self.last_latency.try_into().unwrap());
                packet.write_ticcmd_diff(&send_obj.cmd, lowres_turn);
            }
        }

        self.send_packet(&packet);
        self.need_acknowledge = false;
        debug!("Sent tics from {} to {}", start, end);
    }

    pub fn send_ticcmd(&mut self, ticcmd: &TicCmd, maketic: u32) {
        let mut diff = TicDiff::default();
        self.calculate_ticcmd_diff(ticcmd, &mut diff);

        let sendobj = &mut self.send_queue[maketic as usize % BACKUPTICS];
        sendobj.active = true;
        sendobj.seq = maketic;
        sendobj.time = Instant::now();
        sendobj.cmd = diff;

        let starttic = self.settings.as_ref().map_or(0, |s| {
            if maketic < s.extratics as u32 {
                0
            } else {
                maketic - s.extratics as u32
            }
        });
        let endtic = maketic;

        self.send_tics(starttic, endtic);
    }

    fn calculate_ticcmd_diff(&self, ticcmd: &TicCmd, diff: &mut TicDiff) {
        diff.diff = 0;
        diff.cmd = *ticcmd;

        if self.last_ticcmd.forwardmove != ticcmd.forwardmove {
            diff.diff |= NET_TICDIFF_FORWARD;
        }
        if self.last_ticcmd.sidemove != ticcmd.sidemove {
            diff.diff |= NET_TICDIFF_SIDE;
        }
        if self.last_ticcmd.angleturn != ticcmd.angleturn {
            diff.diff |= NET_TICDIFF_TURN;
        }
        if self.last_ticcmd.buttons != ticcmd.buttons {
            diff.diff |= NET_TICDIFF_BUTTONS;
        }
        if self.last_ticcmd.consistancy != ticcmd.consistancy {
            diff.diff |= NET_TICDIFF_CONSISTANCY;
        }
        if ticcmd.chatchar != 0 {
            diff.diff |= NET_TICDIFF_CHATCHAR;
        } else {
            diff.cmd.chatchar = 0;
        }
        if self.last_ticcmd.lookfly != ticcmd.lookfly || ticcmd.arti != 0 {
            diff.diff |= NET_TICDIFF_RAVEN;
        } else {
            diff.cmd.arti = 0;
        }
        if self.last_ticcmd.buttons2 != ticcmd.buttons2 || ticcmd.inventory != 0 {
            diff.diff |= NET_TICDIFF_STRIFE;
        } else {
            diff.cmd.inventory = 0;
        }
    }

    fn advance_window(&mut self) {
        while self.recv_window[0].active {
            let mut ticcmds = [TicCmd::default(); NET_MAXPLAYERS];
            let window_start = self.recv_window_start;

            let window = self.recv_window[0].cmd;
            self.expand_full_ticcmd(&window, window_start, &mut ticcmds);

            self.receive_tic(&ticcmds, &self.recv_window[0].cmd.playeringame);

            self.recv_window.rotate_left(1);
            self.recv_window[BACKUPTICS - 1] = ServerRecv::default();
            self.recv_window_start += 1;

            debug!("Advanced receive window to {}", self.recv_window_start);
        }
    }

    fn expand_full_ticcmd(
        &mut self,
        cmd: &FullTicCmd,
        _seq: u32,
        ticcmds: &mut [TicCmd; NET_MAXPLAYERS],
    ) {
        let consoleplayer = self
            .settings
            .as_ref()
            .map_or(0, |s| s.consoleplayer as usize);
        let drone = self.drone;
        let mut recvwindow_cmd_base = self.recvwindow_cmd_base;

        for i in 0..NET_MAXPLAYERS {
            if i == consoleplayer && !drone {
                continue;
            }

            if cmd.playeringame[i] {
                let diff = &cmd.cmds[i];
                let mut base = recvwindow_cmd_base[i];
                Self::apply_ticcmd_diff(&mut base, diff, &mut ticcmds[i]);
                recvwindow_cmd_base[i] = ticcmds[i];
            }
        }

        self.recvwindow_cmd_base = recvwindow_cmd_base;
    }

    fn apply_ticcmd_diff(base: &mut TicCmd, diff: &TicDiff, result: &mut TicCmd) {
        *result = *base;

        if diff.diff & NET_TICDIFF_FORWARD != 0 {
            result.forwardmove = diff.cmd.forwardmove;
        }
        if diff.diff & NET_TICDIFF_SIDE != 0 {
            result.sidemove = diff.cmd.sidemove;
        }
        if diff.diff & NET_TICDIFF_TURN != 0 {
            result.angleturn = diff.cmd.angleturn;
        }
        if diff.diff & NET_TICDIFF_BUTTONS != 0 {
            result.buttons = diff.cmd.buttons;
        }
        if diff.diff & NET_TICDIFF_CONSISTANCY != 0 {
            result.consistancy = diff.cmd.consistancy;
        }
        if diff.diff & NET_TICDIFF_CHATCHAR != 0 {
            result.chatchar = diff.cmd.chatchar;
        } else {
            result.chatchar = 0;
        }
        if diff.diff & NET_TICDIFF_RAVEN != 0 {
            result.lookfly = diff.cmd.lookfly;
            result.arti = diff.cmd.arti;
        } else {
            result.arti = 0;
        }
        if diff.diff & NET_TICDIFF_STRIFE != 0 {
            result.buttons2 = diff.cmd.buttons2;
            result.inventory = diff.cmd.inventory;
        } else {
            result.inventory = 0;
        }

        *base = *result;
    }

    fn receive_tic(
        &self,
        _ticcmds: &[TicCmd; NET_MAXPLAYERS],
        playeringame: &[bool; NET_MAXPLAYERS],
    ) {
        // TODO: Implement this.
        debug!(
            "Received tic data for {} players",
            playeringame.iter().filter(|&&p| p).count()
        );
    }

    fn check_resends(&mut self) {
        let now = Instant::now();
        let mut resend_start = -1;
        let mut resend_end = -1;
        let maybe_deadlocked = now.duration_since(self.gamedata_recv_time) > Duration::from_secs(1);

        for i in 0..BACKUPTICS {
            let recvobj = &mut self.recv_window[i];
            let need_resend =
                !recvobj.active && recvobj.resend_time.elapsed() > Duration::from_millis(300);

            if i == 0
                && !recvobj.active
                && recvobj.resend_time.elapsed() > Duration::from_secs(1)
                && maybe_deadlocked
            {
                let _need_resend = true;
            }

            if need_resend {
                if resend_start < 0 {
                    resend_start = i as i32;
                }
                resend_end = i as i32;
            } else if resend_start >= 0 {
                debug!(
                    "Resend request timed out for {}-{}",
                    self.recv_window_start + resend_start as u32,
                    self.recv_window_start + resend_end as u32
                );
                self.send_resend_request(
                    self.recv_window_start + resend_start as u32,
                    self.recv_window_start + resend_end as u32,
                );
                resend_start = -1;
            }
        }

        if resend_start >= 0 {
            debug!(
                "Resend request timed out for {}-{}",
                self.recv_window_start + resend_start as u32,
                self.recv_window_start + resend_end as u32
            );
            self.send_resend_request(
                self.recv_window_start + resend_start as u32,
                self.recv_window_start + resend_end as u32,
            );
        }

        if self.need_acknowledge
            && now.duration_since(self.gamedata_recv_time) > Duration::from_millis(200)
        {
            debug!(
                "No game data received since {:?}: triggering ack",
                self.gamedata_recv_time
            );
            self.send_game_data_ack();
        }
    }

    fn run_bot(&mut self) {
        if self.state == ClientState::InGame && self.drone {
            let maketic = self.recv_window_start + BACKUPTICS as u32;
            let mut bot_ticcmd = TicCmd::default();
            self.generate_bot_ticcmd(&mut bot_ticcmd);
            self.send_ticcmd(&bot_ticcmd, maketic);
        }
    }

    fn generate_bot_ticcmd(&self, ticcmd: &mut TicCmd) {
        // TODO: Implement more sophisticated bot AI logic
        ticcmd.forwardmove = 50;
        ticcmd.sidemove = 0;
        ticcmd.angleturn = 0;
    }

    pub fn disconnect(&mut self) {
        if !self.net_client_connected {
            return;
        }

        info!("Beginning disconnect");
        self.state = ClientState::Disconnecting;
        self.start_time = Instant::now();

        // Send disconnect packet five times
        for _ in 0..5 {
            let mut packet = Packet::new();
            packet.write_u16(PacketType::Disconnect.to_u16());
            self.send_packet(&packet);
        }

        self.state = ClientState::Disconnected;
        self.shutdown();
        info!("Disconnect complete");
    }

    pub fn get_settings(&self) -> Option<GameSettings> {
        if self.state != ClientState::InGame {
            return None;
        }
        self.settings
    }

    pub fn launch_game(&mut self) {
        let packet = self.new_reliable_packet(PacketType::Launch);
        self.send_packet(&packet);
    }

    pub fn start_game(&mut self, settings: &GameSettings) {
        self.last_ticcmd = TicCmd::default();

        let mut packet = self.new_reliable_packet(PacketType::GameStart);
        packet.write_settings(settings);
        self.send_packet(&packet);
    }

    fn send_packet(&self, packet: &Packet) {
        if let Some(server_addr) = self.server_addr {
            if let Err(e) = self.socket.send_to(&packet.data, server_addr) {
                warn!("Failed to send packet: {}", e);
            }
        }
    }

    pub fn connect<A: ToSocketAddrs>(
        &mut self,
        addr: A,
        connect_data: ConnectData,
    ) -> Result<(), String> {
        let addr = addr
            .to_socket_addrs()
            .map_err(|e| format!("Failed to resolve address: {}", e))?
            .next()
            .ok_or_else(|| "No valid address found".to_string())?;
        info!("Attempting to connect to server at {:?}", addr);
        self.server_addr = Some(addr);

        self.state = ClientState::Connecting;
        self.reject_reason = Some("Unknown reason".to_string());

        self.net_local_wad_sha1sum
            .copy_from_slice(&connect_data.wad_sha1sum);
        self.net_local_deh_sha1sum
            .copy_from_slice(&connect_data.deh_sha1sum);
        self.net_local_is_freedoom = connect_data.is_freedoom != 0;

        self.gamemode = connect_data.gamemode;
        self.gamemission = connect_data.gamemission;
        self.lowres_turn = connect_data.lowres_turn;
        self.max_players = connect_data.max_players;
        self.is_freedoom = connect_data.is_freedoom;
        self.player_class = connect_data.player_class;

        self.net_client_connected = false;
        self.net_client_received_wait_data = false;

        self.start_time = Instant::now();
        self.last_send_time = Instant::now() - KEEPALIVE_PERIOD;
        self.num_retries = 0;

        while self.state == ClientState::Connecting {
            if self.start_time.elapsed() > CONNECTION_TIMEOUT {
                return Err(format!(
                    "Connection timed out after {} seconds",
                    CONNECTION_TIMEOUT.as_secs()
                ));
            }

            if self.num_retries >= MAX_RETRIES {
                return Err(format!("Connection failed after {} retries", MAX_RETRIES));
            }

            info!("Sending SYN packet, attempt {}", self.num_retries + 1);
            self.send_syn(&connect_data);

            self.num_retries += 1;

            for _ in 0..10 {
                self.run();
                let reject_reason = self.reject_reason.clone();

                if self.state == ClientState::Connected {
                    break;
                } else if let Some(reject_reason) = reject_reason {
                    self.disconnect();
                    return Err(format!("Connection rejected: {}", reject_reason));
                }

                thread::sleep(Duration::from_millis(200));
            }

            if self.state == ClientState::Connected {
                info!("Successfully connected");
                self.reject_reason = None;
                self.state = ClientState::WaitingLaunch;
                self.drone = connect_data.drone != 0;
                self.net_client_connected = true;
                return Ok(());
            }

            info!(
                "Connection attempt {} failed, retrying...",
                self.num_retries
            );
            thread::sleep(Duration::from_secs(2));
        }

        Err(format!(
            "Connection failed. Reason: {:?}",
            self.reject_reason
        ))
    }

    fn send_syn(&mut self, connect_data: &ConnectData) {
        let mut packet = Packet::new();

        // 1. Packet Type (SYN)
        packet.write_u16(PacketType::Syn.to_u16());

        // 2. Random Challenge
        packet.write_u32(rand::random());

        // 3. Game Description
        packet.write_string("Chocolate Doom 3.0.1");

        // 4. Number of Protocols
        packet.write_u8(1);

        // 5. Protocol Identifier
        packet.write_string("CHOCOLATE_DOOM_0");

        // 6. Calculate Data Length
        let player_name_len = self.player_name.len() + 1; // +1 for null terminator
        let data_length = 6 + 20 + 20 + 1 + player_name_len; // Should be around 56 bytes

        // Write Data Length in Little-Endian
        packet.write_u32(data_length as u32);

        // 7. Connect Data
        // Game Parameters
        // 7. Connect Data
        packet.write_u8(connect_data.gamemode as u8);
        packet.write_u8(connect_data.gamemission as u8);
        packet.write_u8(connect_data.lowres_turn as u8);
        packet.write_u8(connect_data.drone as u8);
        packet.write_u8(connect_data.max_players as u8);
        packet.write_u8(connect_data.is_freedoom as u8);
        packet.write_blob(&connect_data.wad_sha1sum);
        packet.write_blob(&connect_data.deh_sha1sum);
        packet.write_u8(connect_data.player_class as u8);
        packet.write_string(&self.player_name);

        // Send the packet
        self.send_packet(&packet);
        info!("SYN sent to server: {} bytes", packet.data.len());
    }

    pub fn build_ticcmd(&mut self, cmd: &mut TicCmd, _maketic: u32) {
        // TODO: Implement actual ticcmd building logic
        *cmd = TicCmd::default();
    }

    pub fn run_tic(&mut self, _cmds: &[TicCmd; NET_MAXPLAYERS], _ingame: &[bool; NET_MAXPLAYERS]) {
        // TODO: Implement actual tic running logic
        // Commented out for now to avoid unused variable warnings
        // for (i, (cmd, &in_game)) in _cmds.iter().zip(_ingame.iter()).enumerate() {
        //     if in_game {
        //         debug!("Player {}: {:?}", i, cmd);
        //     }
        // }
    }

    pub fn is_drone(&self) -> bool {
        self.drone
    }

    pub fn is_connected(&self) -> bool {
        self.net_client_connected
    }

    fn new_reliable_packet(&mut self, packet_type: PacketType) -> Packet {
        let mut packet = Packet::new();
        packet.write_u16(packet_type.to_u16() | NET_RELIABLE_PACKET);
        packet.write_u8(self.reliable_send_seq);

        self.reliable_packets.push(ReliablePacket {
            packet: packet.clone(),
            seq: self.reliable_send_seq,
            last_send_time: Instant::now(),
        });
        self.reliable_send_seq = self.reliable_send_seq.wrapping_add(1);

        packet
    }

    pub fn request_launch(&mut self) {
        if self.state == ClientState::WaitingLaunch {
            let mut packet = Packet::new();
            packet.write_u16(PacketType::Launch.to_u16() | NET_RELIABLE_PACKET);
            packet.write_u8(self.reliable_send_seq);
            self.send_packet(&packet);
            debug!("Sent launch request: {:x?}", packet.data);
            self.reliable_send_seq = self.reliable_send_seq.wrapping_add(1);
        }
    }
}
