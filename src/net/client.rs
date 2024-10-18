use rand::prelude::*;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};
use std::{io, thread};
use tracing::{debug, error, info, warn};

use crate::bot::Bot;

use super::packet::Packet;
use super::*;

const PACKAGE_STRING: &str = "Chocolate Doom 3.0.1";
const KEEPALIVE_PERIOD: Duration = Duration::from_secs(1);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

struct PIDController {
    kp: f32,
    ki: f32,
    kd: f32,
    integral: i32,
    previous_error: i32,
}

impl PIDController {
    fn new(kp: f32, ki: f32, kd: f32) -> Self {
        PIDController {
            kp,
            ki,
            kd,
            integral: 0,
            previous_error: 0,
        }
    }

    fn update(&mut self, error: i32) -> i32 {
        let p = self.kp * error as f32;
        self.integral += error;
        let i = self.ki * self.integral as f32;
        let d = self.kd * (error - self.previous_error) as f32;
        self.previous_error = error;
        (p + i + d) as i32
    }
}

pub struct Client {
    bot: Bot,

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
    net_waiting_for_launch: bool,
    net_client_connected: bool,
    net_client_received_wait_data: bool,
    net_client_wait_data: WaitData,
    last_send_time: Instant,
    last_ticcmd: TicCmd,
    start_time: Instant,
    protocol: Protocol,
    is_freedoom: u8,
    pid_controller: PIDController,
    game_time_offset: i32,
    gametic: i32,
    last_gamedata_time: Instant,
    reliable_packets: std::collections::HashMap<u32, Packet>,
    next_reliable_seq: u32,
}

impl Client {
    pub fn new(player_name: String) -> io::Result<Self> {
        info!("Creating new Client");

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(Client {
            bot: Bot::new(None),
            socket,
            state: ClientState::Disconnected,
            server_addr: None,
            settings: None,
            reject_reason: None,
            player_name,
            drone: false,
            recv_window_start: 0,
            recv_window: [ServerRecv::default(); BACKUPTICS],
            send_queue: [ServerSend::default(); BACKUPTICS],
            need_acknowledge: false,
            gamedata_recv_time: Instant::now(),
            last_latency: 0,
            net_waiting_for_launch: false,
            net_client_connected: false,
            net_client_received_wait_data: false,
            net_client_wait_data: WaitData::default(),
            last_send_time: Instant::now(),
            last_ticcmd: TicCmd::default(),
            start_time: Instant::now(),
            protocol: Protocol::ChocolateDoom0,
            is_freedoom: 0,
            pid_controller: PIDController::new(0.1, 0.001, 0.05),
            game_time_offset: 0,
            gametic: 0,
            last_gamedata_time: Instant::now(),
            reliable_packets: std::collections::HashMap::new(),
            next_reliable_seq: 0,
        })
    }

    pub fn get_reject_reason(&self) -> Option<&str> {
        self.reject_reason.as_deref()
    }

    pub fn init(&mut self) {
        debug!("Initializing Client");

        self.bot.init();
        self.net_client_connected = false;
        self.net_client_received_wait_data = false;
        self.net_waiting_for_launch = false;

        if self.player_name.is_empty() {
            self.player_name = Self::get_player_name();
            debug!("Processing resend request");
            debug!("Player name set to: {}", self.player_name);
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

    pub fn build_and_send_tic(&mut self, maketic: u32) {
        if maketic % self.settings.as_ref().map_or(1, |s| s.ticdup as u32) != 0 {
            return;
        }

        let mut ticcmd = TicCmd::default();
        self.bot.build_ticcmd(&mut ticcmd, maketic);

        let mut diff = TicDiff::default();
        self.calculate_ticcmd_diff(&ticcmd, &mut diff);

        if !self.drone {
            let sendobj = &mut self.send_queue[maketic as usize % BACKUPTICS];
            sendobj.active = true;
            sendobj.seq = maketic;
            sendobj.time = Instant::now();
            sendobj.cmd = diff;

            let starttic =
                maketic.saturating_sub(self.settings.as_ref().map_or(0, |s| s.extratics as u32));
            let endtic = maketic;

            self.send_tics(starttic, endtic);
        }

        self.last_ticcmd = ticcmd;
    }

    pub fn run(&mut self) {
        self.receive_packets();
        self.handle_state();

        if self.need_acknowledge {
            self.send_game_data_ack();
        }

        self.handle_state();
        self.send_keepalive();
        self.check_resends();

        if self.state == ClientState::InGame {
            self.build_and_send_tic(self.gametic as u32);
        }
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
            ClientState::WaitingLaunch => self.handle_waiting_launch(),
            ClientState::WaitingStart => self.handle_waiting_start(),
            ClientState::InGame => self.handle_in_game(),
            ClientState::Disconnecting => self.handle_disconnecting(),
            _ => {}
        }
    }

    fn handle_connecting(&mut self) {
        let elapsed = self.start_time.elapsed();
        debug!("Connecting... Time elapsed: {:?}", elapsed);
        if elapsed > CONNECTION_TIMEOUT {
            self.handle_connection_timeout();
        }
    }

    fn handle_waiting_launch(&mut self) {
        self.net_waiting_for_launch = true;
        debug!("Waiting for launch");
    }

    fn handle_waiting_start(&mut self) {
        let settings_clone = self.settings;
        if let Some(settings) = settings_clone {
            self.send_game_start(&settings);
        }
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
        info!("Disconnected from server");

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
        let packet_type = packet.read_u16().and_then(PacketType::from_u16);

        match packet_type {
            Some(PacketType::Syn) => self.parse_syn(packet),
            Some(PacketType::Ack) => self.parse_ack(packet),
            Some(PacketType::Rejected) => self.parse_reject(packet),
            Some(PacketType::WaitingData) => self.parse_waiting_data(packet),
            Some(PacketType::Launch) => self.parse_launch(packet),
            Some(PacketType::GameStart) => self.parse_game_start(packet),
            Some(PacketType::GameData) => self.parse_game_data(packet),
            Some(PacketType::GameDataResend) => self.parse_resend_request(packet),
            Some(PacketType::ConsoleMessage) => self.parse_console_message(packet),
            Some(PacketType::Disconnect) => self.parse_disconnect(packet),
            Some(PacketType::DisconnectAck) => self.parse_disconnect_ack(packet),
            Some(PacketType::KeepAlive) => debug!("Received keep-alive packet"),
            _ => warn!("Unknown packet type: {:x?}", original_data),
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

        if let Some(magic) = packet.read_u32() {
            if magic != NET_MAGIC_NUMBER {
                error!("Incorrect magic number in SYN packet");
                return;
            }
        } else {
            error!("Failed to read magic number from SYN packet");
            return;
        }

        if let Some(server_version) = packet.read_safe_string() {
            debug!("Server version: {}", server_version);

            if let Some(protocol) = self.negotiate_protocol(packet) {
                self.protocol = protocol;
                info!("Negotiated protocol: {:?}", protocol);

                // Set the connection state to CONNECTED
                self.state = ClientState::Connected;

                // Send an ACK packet in response to the SYN
                self.send_ack();

                // Check for version mismatch
                if server_version != PACKAGE_STRING {
                    warn!(
                        "Version mismatch: Client is '{}', but the server is '{}'. \
                        It is possible that this mismatch may cause the game to desync.",
                        PACKAGE_STRING, server_version
                    );
                }
            } else {
                error!("Failed to negotiate a common protocol");
                self.reject_reason = Some("No common protocol".to_string());
            }
        } else {
            error!("Failed to read server version");
            self.reject_reason = Some("Failed to read server version".to_string());
        }
    }

    fn send_ack(&mut self) {
        let mut ack_packet = Packet::new();
        ack_packet.write_u16(PacketType::Ack.to_u16());
        ack_packet.write_protocol(self.protocol);
        self.send_packet(&ack_packet);
        info!("ACK sent to server");
    }

    fn negotiate_protocol(&self, packet: &mut Packet) -> Option<Protocol> {
        if let Some(protocol) = packet.read_protocol() {
            if protocol != Protocol::Unknown {
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

    fn send_disconnect_ack(&mut self, packet: &mut Packet) {
        packet.write_u16(PacketType::DisconnectAck.to_u16());
        packet.write_u32(0x80);
        self.send_packet(packet);
    }

    fn parse_waiting_data(&mut self, packet: &mut Packet) {
        if let Some(wait_data) = packet.read_wait_data() {
            if self.validate_wait_data(&wait_data) {
                self.net_client_wait_data = wait_data;
                self.net_client_received_wait_data = true;

                debug!("Received waiting data: {:?}", self.net_client_wait_data);

                self.is_freedoom = self.net_client_wait_data.is_freedoom as u8;

                self.send_ack();
            }
        }
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
            if settings.num_players > NET_MAXPLAYERS as i32
                || settings.consoleplayer >= settings.num_players
            {
                error!("Invalid game settings received: {:?}", settings);
                return;
            }

            if (self.drone && settings.consoleplayer >= 0)
                || (!self.drone && settings.consoleplayer < 0)
            {
                error!("Mismatch in drone status and consoleplayer");
                return;
            }

            self.settings = Some(settings);
            self.state = ClientState::InGame;
            self.initialize_game_state();
        } else {
            error!("Failed to read game settings from GameStart packet");
        }
    }

    fn initialize_game_state(&mut self) {
        self.recv_window_start = 0;
        self.recv_window = [ServerRecv::default(); BACKUPTICS];
        self.send_queue = [ServerSend::default(); BACKUPTICS];
    }

    fn parse_game_data(&mut self, packet: &mut Packet) {
        debug!("Processing game data packet");
        if let (Some(seq_byte), Some(num_tics)) = (packet.read_u8(), packet.read_u8()) {
            // Set need_to_acknowledge to true
            self.need_acknowledge = true;
            self.last_gamedata_time = Instant::now();
            let seq = self.expand_tic_num(seq_byte as u32);
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
        } else {
            error!("Failed to read sequence number or number of tics");
        }
    }

    fn store_received_tic(&mut self, seq: u32, cmd: FullTicCmd) {
        let index = (seq - self.recv_window_start) as usize;
        if index < BACKUPTICS {
            self.recv_window[index].active = true;
            self.recv_window[index].cmd = cmd;
            debug!("Stored tic {} in receive window", seq);
            self.update_clock_sync(seq, cmd.latency);
        } else {
            warn!("Received tic {} is outside of receive window", seq);
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

    pub fn send_tics(&mut self, start: u32, end: u32) {
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
                packet.write_i16(self.last_latency as i16);
                packet.write_ticcmd_diff(&send_obj.cmd, lowres_turn);
            }
        }

        self.send_packet(&packet);
        self.need_acknowledge = false;
        debug!("Sent tics from {} to {}", start, end);
    }

    fn advance_window(&mut self) {
        while self.recv_window[0].active {
            let mut ticcmds = [TicCmd::default(); NET_MAXPLAYERS];
            let window_start = self.recv_window_start;

            let window = self.recv_window[0].cmd;
            self.expand_full_ticcmd(&window, window_start, &mut ticcmds);

            let playeringame = self.recv_window[0].cmd.playeringame;
            self.receive_tic(&ticcmds, &playeringame);

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

        for (i, ticcmd) in ticcmds.iter_mut().enumerate().take(NET_MAXPLAYERS) {
            if i == consoleplayer && !drone {
                continue;
            }

            if cmd.playeringame[i] {
                let diff = &cmd.cmds[i];
                let mut base = self.bot.recvwindow_cmd_base[i];
                Self::apply_ticcmd_diff(&mut base, diff, ticcmd);
                self.bot.recvwindow_cmd_base[i] = *ticcmd;
            }
        }
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
        &mut self,
        ticcmds: &[TicCmd; NET_MAXPLAYERS],
        playeringame: &[bool; NET_MAXPLAYERS],
    ) {
        for (i, (&cmd, &ingame)) in ticcmds.iter().zip(playeringame.iter()).enumerate() {
            if ingame {
                self.bot.recvwindow_cmd_base[i] = cmd;
            }
        }

        // advance game tic
        self.gametic += 1;

        debug!(
            "Received tic {}, player states: {:?}",
            self.gametic, playeringame
        );
    }

    fn check_resends(&mut self) {
        let now = Instant::now();
        let deadlock_timeout = Duration::from_secs(1);

        if now.duration_since(self.last_gamedata_time) > deadlock_timeout {
            // deadlock prevention
            for i in 0..BACKUPTICS {
                let recvobj = &mut self.recv_window[i];

                if !recvobj.active && recvobj.resend_time == Instant::now() {
                    // send resend request for missing tic
                    let start = self.recv_window_start + i as u32;
                    let end = start + 5; // request 5 tics

                    self.send_resend_request(start, end);
                    self.last_gamedata_time = now;
                    break;
                }
            }
        }

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

    pub fn get_settings(&self) -> Option<GameSettings> {
        if self.state != ClientState::InGame {
            return None;
        }
        self.settings
    }

    fn send_packet(&mut self, packet: &Packet) {
        let is_reliable = matches!(
            PacketType::from_u16(u16::from_le_bytes([packet.data[0], packet.data[1]])),
            Some(PacketType::Syn)
                | Some(PacketType::Launch)
                | Some(PacketType::GameStart)
                | Some(PacketType::Disconnect)
        );

        if is_reliable {
            self.reliable_packets
                .insert(self.next_reliable_seq, packet.clone());
            self.next_reliable_seq += 1;
        }
        if let Some(server_addr) = self.server_addr {
            if let Err(e) = self.socket.send_to(&packet.data, server_addr) {
                warn!("Failed to send packet: {}", e);
            }
        }
    }

    pub fn connect<A: ToSocketAddrs>(
        &mut self,
        addr: A,
        mut connect_data: ConnectData,
    ) -> Result<(), String> {
        // Ensure max_players is set to 4
        connect_data.max_players = 4;
        let addr = addr
            .to_socket_addrs()
            .map_err(|e| format!("Failed to resolve address: {}", e))?
            .next()
            .ok_or_else(|| "No valid address found".to_string())?;

        self.server_addr = Some(addr);
        self.state = ClientState::Connecting;

        let start_time = Instant::now();
        let mut last_send_time = Instant::now() - Duration::from_secs(1);

        let mut syn_sent = false;
        while self.state == ClientState::Connecting {
            let now = Instant::now();

            if now.duration_since(start_time) > Duration::from_secs(5) {
                return Err("Connection timed out".to_string());
            }

            if !syn_sent || now.duration_since(last_send_time) >= Duration::from_secs(1) {
                self.send_syn(&connect_data, &mut Packet::new());
                last_send_time = now;
                syn_sent = true;
            }

            self.receive_packets();

            if self.state != ClientState::Connecting {
                break; // Exit the loop if we've received a response
            }

            thread::sleep(Duration::from_millis(10));
        }

        if self.state == ClientState::Connected {
            Ok(())
        } else {
            Err(self
                .reject_reason
                .clone()
                .unwrap_or_else(|| "Connection failed".to_string()))
        }
    }

    fn send_syn(&mut self, connect_data: &ConnectData, packet: &mut Packet) {
        packet.write_u16(PacketType::Syn.to_u16());
        packet.write_u32(0x56abe18c);
        packet.write_string(PACKAGE_STRING);
        packet.write_u8(1); // Number of protocols
        packet.write_string("CHOCOLATE_DOOM_0");
        packet.write_connect_data(connect_data);
        packet.write_string(&self.player_name);

        self.send_packet(packet);
    }

    pub fn run_tic(&mut self, cmds: &[TicCmd; NET_MAXPLAYERS], ingame: &[bool; NET_MAXPLAYERS]) {
        for (i, (&cmd, &in_game)) in cmds.iter().zip(ingame.iter()).enumerate() {
            if in_game {
                self.apply_command(i, &cmd);
            }
        }

        self.update_world();

        debug!(
            "Ran tic, applied commands for {} players",
            ingame.iter().filter(|&&x| x).count()
        );
    }

    pub fn calculate_ticcmd_diff(&self, ticcmd: &TicCmd, diff: &mut TicDiff) {
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

    fn apply_command(&mut self, player_num: usize, cmd: &TicCmd) {
        debug!("Applied command for player {}: {:?}", player_num, cmd);
    }

    fn update_world(&mut self) {
        debug!("Updated world state");
    }

    pub fn is_drone(&self) -> bool {
        self.drone
    }

    pub fn is_connected(&self) -> bool {
        self.net_client_connected
    }

    fn parse_ack(&mut self, packet: &mut Packet) {
        debug!("Processing ACK response");
        if self.state == ClientState::Connecting {
            if let Some(server_version) = packet.read_safe_string() {
                debug!("Server version: {}", server_version);
                if let Some(protocol) = self.negotiate_protocol(packet) {
                    self.protocol = protocol;
                    info!("Connected to server using protocol: {:?}", protocol);
                    self.state = ClientState::Connected;
                } else {
                    error!("No common protocol found during negotiation");
                    self.reject_reason = Some("No common protocol".to_string());
                }
            } else {
                error!("Failed to read server version");
                self.reject_reason = Some("Failed to read server version".to_string());
            }
        }
    }

    fn send_game_start(&mut self, settings: &GameSettings) {
        let mut packet = Packet::new();
        packet.write_u16(PacketType::GameStart.to_u16());
        packet.write_settings(settings);
        self.send_packet(&packet);
        info!("GameStart sent to server");
    }

    pub fn disconnect(&mut self) {
        if self.state != ClientState::Disconnected {
            self.state = ClientState::Disconnecting;
            let mut packet = Packet::new();
            packet.write_u16(PacketType::Disconnect.to_u16());
            self.send_packet(&packet);
            info!("Disconnect request sent to server");
        }
    }
}
impl Client {
    fn update_clock_sync(&mut self, seq: u32, remote_latency: i32) {
        let now = Instant::now();
        let send_time = self.send_queue[seq as usize % BACKUPTICS].time;
        let latency = now.duration_since(send_time).as_millis() as i32;

        let error = latency - remote_latency;
        let adjustment = self.pid_controller.update(error);
        self.game_time_offset += adjustment;

        self.last_latency = latency;

        debug!(
            "Clock sync: latency={}, remote_latency={}, error={}, adjustment={}, game_time_offset={}",
            latency, remote_latency, error, adjustment, self.game_time_offset
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::ConnectData;

    #[test]
    fn test_syn_message() {
        // Create a client
        let mut client = Client::new("hydra-bot".to_string()).unwrap();

        // Create connect data
        let connect_data = ConnectData {
            gamemode: 1,
            gamemission: 0,
            lowres_turn: 0,
            drone: 0,
            max_players: 4,
            is_freedoom: 0,
            wad_sha1sum: [
                0x77, 0x42, 0x08, 0x9b, 0x44, 0x68, 0xa7, 0x36, 0xca, 0xdb, 0x65, 0x9a, 0x7d, 0xec,
                0xa3, 0x32, 0x0f, 0xe6, 0xdc, 0xbd,
            ],
            deh_sha1sum: [0x00; 20],
            player_class: 22,
        };

        let mut packet = Packet::new();
        client.send_syn(&connect_data, &mut packet);

        let hex_string = packet.data.iter().fold(String::new(), |mut acc, &b| {
            write!(acc, "{:02x}", b).unwrap();
            acc
        });

        // Expected hexadecimal string
        let expected = "00008ce1ab5643686f636f6c61746520446f6f6d20332e302e31000143484f434f4c4154455f444f4f4d5f30000100000004007742089b4468a736cadb659a7deca3320fe6dcbd000000000000000000000000000000000000000016706366636f73746100";

        // Assert that the generated packet matches the expected string
        assert_eq!(
            hex_string, expected,
            "SYN message does not match expected format"
        );
    }
}
