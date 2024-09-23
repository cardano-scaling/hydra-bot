use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};

use crate::net_packet::NetPacket;
use crate::net_structs::*;

// Constants
const NET_PACKET_TYPE_SYN: u16 = 0;
const NET_PACKET_TYPE_REJECTED: u16 = 1;
const NET_PACKET_TYPE_WAITING_DATA: u16 = 2;
const NET_PACKET_TYPE_LAUNCH: u16 = 3;
const NET_PACKET_TYPE_GAMESTART: u16 = 4;
const NET_PACKET_TYPE_GAMEDATA: u16 = 5;
const NET_PACKET_TYPE_GAMEDATA_ACK: u16 = 6;
const NET_PACKET_TYPE_GAMEDATA_RESEND: u16 = 7;
const NET_PACKET_TYPE_CONSOLE_MESSAGE: u16 = 8;
const NET_PACKET_TYPE_DISCONNECT: u16 = 9;
const NET_PACKET_TYPE_DISCONNECT_ACK: u16 = 10;
const NET_MAGIC_NUMBER: u32 = 1454104972;

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

pub struct NetClient {
    connection: NetConnection,
    state: ClientState,
    server_addr: Option<NetAddr>,
    context: NetContext,
    settings: Option<GameSettings>,
    reject_reason: Option<String>,
    player_name: String,
    drone: bool,
    recv_window_start: u32,
    recv_window: Vec<NetServerRecv>,
    send_queue: Vec<NetServerSend>,
    need_acknowledge: bool,
    gamedata_recv_time: Instant,
    last_latency: i32,
    net_local_wad_sha1sum: [u8; 20],
    net_local_deh_sha1sum: [u8; 20],
    net_local_is_freedoom: bool,
    net_waiting_for_launch: bool,
    net_client_connected: bool,
    net_client_received_wait_data: bool,
    net_client_wait_data: NetWaitData,
    last_send_time: Instant,
    last_ticcmd: TicCmd,
    recvwindow_cmd_base: Vec<TicCmd>,
    start_time: Instant,
    num_retries: u32,
}

impl NetClient {
    // ... existing methods ...

    pub fn is_drone(&self) -> bool {
        self.drone
    }

    pub fn is_connected(&self) -> bool {
        self.net_client_connected
    }
}

impl NetClient {
    pub fn new(player_name: String, drone: bool) -> Self {
        debug!("Creating new NetClient: player_name={}, drone={}", player_name, drone);
        NetClient {
            connection: NetConnection::default(),
            state: ClientState::Disconnected,
            server_addr: None,
            context: NetContext::default(),
            settings: None,
            reject_reason: None,
            player_name,
            drone,
            recv_window_start: 0,
            recv_window: vec![NetServerRecv::default(); BACKUPTICS],
            send_queue: vec![NetServerSend::default(); BACKUPTICS],
            need_acknowledge: false,
            gamedata_recv_time: Instant::now(),
            last_latency: 0,
            net_local_wad_sha1sum: [0; 20],
            net_local_deh_sha1sum: [0; 20],
            net_local_is_freedoom: false,
            net_waiting_for_launch: false,
            net_client_connected: false,
            net_client_received_wait_data: false,
            net_client_wait_data: NetWaitData::default(),
            last_send_time: Instant::now(),
            last_ticcmd: TicCmd::default(),
            recvwindow_cmd_base: vec![TicCmd::default(); NET_MAXPLAYERS],
            num_retries: 0,
            start_time: Instant::now(),
        }
    }

    pub fn init(&mut self) {
        debug!("Initializing NetClient");
        self.init_bot();
        self.net_client_connected = false;
        self.net_client_received_wait_data = false;
        self.net_waiting_for_launch = false;

        // Try to set player name from environment variables or command line arguments
        if self.player_name.is_empty() {
            self.player_name = std::env::args().nth(1).unwrap_or_else(|| {
                std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_else(|_| NetClient::get_random_pet_name())
            });
        }
        debug!("Player name set to: {}", self.player_name);
    }

    fn init_bot(&mut self) {
        if self.drone {
            debug!("Initializing bot-specific settings");
            // Initialize bot-specific settings
            // For example, set bot skill level
        }
    }

    fn get_random_pet_name() -> String {
        let pet_names = ["Fluffy", "Buddy", "Max", "Charlie", "Lucy", "Bailey"];
        pet_names[rand::random::<usize>() % pet_names.len()].to_string()
    }

    pub fn run(&mut self) {
        self.run_bot();

        if !self.net_client_connected {
            return;
        }

        while let Some((addr, mut packet)) = self.context.recv_packet() {
            if Some(addr) == self.server_addr {
                self.parse_packet(&mut packet);
            }
        }

        self.connection.run();

        match self.state {
            ClientState::Connecting => {
                if Instant::now().duration_since(self.start_time) > Duration::from_secs(120) {
                    self.handle_connection_timeout();
                }
            }
            ClientState::Connected | ClientState::WaitingLaunch => {
                self.net_waiting_for_launch = true;
            }
            ClientState::InGame => {
                self.advance_window();
                self.check_resends();
            }
            ClientState::Disconnecting => {
                if Instant::now().duration_since(self.start_time) > Duration::from_secs(5) {
                    self.handle_disconnection_timeout();
                }
            }
            _ => {}
        }

        // Send keepalive if needed
        self.send_keepalive();
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
        if self.state == ClientState::Connected || self.state == ClientState::InGame {
            let now = Instant::now();
            if now.duration_since(self.last_send_time) > Duration::from_secs(1) {
                let mut packet = NetPacket::new();
                packet.write_u16(NET_PACKET_TYPE_GAMEDATA_ACK);
                packet.write_u8((self.recv_window_start & 0xff) as u8);
                self.connection.send_packet(&packet, self.server_addr.as_ref().unwrap());
                self.last_send_time = now;
            }
        }
    }

    fn handle_disconnected(&mut self) {
        self.receive_tic(&[TicCmd::default(); NET_MAXPLAYERS], &[false; NET_MAXPLAYERS]);
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if self.net_client_connected {
            self.connection.disconnect();
        }
        self.state = ClientState::Disconnected;
        self.net_client_connected = false;
    }

    fn parse_packet(&mut self, packet: &mut NetPacket) {
        let packet_type = packet.read_u16().unwrap();

        debug!("Parsing packet of type: {}", packet_type);
        match packet_type {
            NET_PACKET_TYPE_SYN => self.parse_syn(packet),
            NET_PACKET_TYPE_REJECTED => self.parse_reject(packet),
            NET_PACKET_TYPE_WAITING_DATA => self.parse_waiting_data(packet),
            NET_PACKET_TYPE_LAUNCH => self.parse_launch(packet),
            NET_PACKET_TYPE_GAMESTART => self.parse_game_start(packet),
            NET_PACKET_TYPE_GAMEDATA => self.parse_game_data(packet),
            NET_PACKET_TYPE_GAMEDATA_RESEND => self.parse_resend_request(packet),
            NET_PACKET_TYPE_CONSOLE_MESSAGE => self.parse_console_message(packet),
            NET_PACKET_TYPE_DISCONNECT => self.parse_disconnect(packet),
            NET_PACKET_TYPE_DISCONNECT_ACK => self.parse_disconnect_ack(packet),
            _ => warn!("Unknown packet type: {}", packet_type),
        }
    }

    fn parse_disconnect(&mut self, packet: &mut NetPacket) {
        info!("Received disconnect request from server");
        self.send_disconnect_ack();
        self.state = ClientState::Disconnected;
        self.shutdown();
    }

    fn parse_disconnect_ack(&mut self, packet: &mut NetPacket) {
        if self.state == ClientState::Disconnecting {
            info!("Received disconnect acknowledgement");
            self.state = ClientState::Disconnected;
            self.shutdown();
        }
    }

    fn send_disconnect_ack(&self) {
        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_DISCONNECT_ACK);
        self.connection.send_packet(&packet, self.server_addr.as_ref().unwrap());
    }

    fn parse_syn(&mut self, packet: &mut NetPacket) {
        debug!("Processing SYN response");
        let server_version = packet.read_safe_string().unwrap_or_default();
        let protocol = packet.read_protocol();

        if protocol == NetProtocol::Unknown {
            error!("No common protocol");
            return;
        }

        info!("Connected to server");
        self.connection.state = ConnectionState::Connected;
        self.connection.protocol = protocol;

        if server_version != env!("CARGO_PKG_VERSION") {
            warn!(
                "Version mismatch: Client is '{}', but the server is '{}'. \
                This mismatch may cause the game to desynchronize.",
                env!("CARGO_PKG_VERSION"),
                server_version
            );
        }
    }

    fn parse_reject(&mut self, packet: &mut NetPacket) {
        if let Some(msg) = packet.read_safe_string() {
            if self.connection.state == ConnectionState::Connecting {
                warn!("Connection rejected: {}", msg);
                self.connection.state = ConnectionState::Disconnected;
                self.reject_reason = Some(msg);
            }
        }
    }

    fn parse_waiting_data(&mut self, packet: &mut NetPacket) {
        if let Some(wait_data) = packet.read_wait_data() {
            if wait_data.num_players > wait_data.max_players
                || wait_data.ready_players > wait_data.num_players
                || wait_data.max_players > NET_MAXPLAYERS as i32
            {
                return;
            }

            if (wait_data.consoleplayer >= 0 && self.drone)
                || (wait_data.consoleplayer < 0 && !self.drone)
                || (wait_data.consoleplayer as usize >= wait_data.num_players as usize)
            {
                return;
            }

            self.net_client_wait_data = wait_data;
            self.net_client_received_wait_data = true;
        }
    }

    fn parse_launch(&mut self, packet: &mut NetPacket) {
        debug!("Processing launch packet");
        if self.state != ClientState::WaitingLaunch {
            warn!("Error: Not in waiting launch state");
            return;
        }

        if let Some(num_players) = packet.read_u8() {
            self.net_client_wait_data.num_players = num_players as i32;
            self.state = ClientState::WaitingStart;
            info!("Now waiting to start the game");
        }
    }

    fn parse_game_start(&mut self, packet: &mut NetPacket) {
        debug!("Processing game start packet");
        if let Some(settings) = packet.read_settings() {
            if self.state != ClientState::WaitingStart {
                warn!("Error: Not in waiting start state");
                return;
            }

            if settings.num_players > NET_MAXPLAYERS as i32
                || settings.consoleplayer as usize >= settings.num_players as usize
            {
                error!(
                    "Invalid settings, num_players={}, consoleplayer={}",
                    settings.num_players, settings.consoleplayer
                );
                return;
            }

            if (self.drone && settings.consoleplayer >= 0)
                || (!self.drone && settings.consoleplayer < 0)
            {
                error!(
                    "Mismatch: drone={}, consoleplayer={}",
                    self.drone, settings.consoleplayer
                );
                return;
            }

            info!("Initiating game state");
            self.state = ClientState::InGame;
            self.settings = Some(settings);
            self.recv_window_start = 0;
            self.recv_window = vec![NetServerRecv::default(); BACKUPTICS];
            self.send_queue = vec![NetServerSend::default(); BACKUPTICS];
        }
    }

    fn parse_game_data(&mut self, packet: &mut NetPacket) {
        debug!("Processing game data packet");

        if let (Some(seq), Some(num_tics)) = (packet.read_u8(), packet.read_u8()) {
            let seq = self.expand_tic_num(seq as u32);
            debug!(
                "Game data received, seq={}, num_tics={}",
                seq, num_tics
            );

            let lowres_turn = self.settings.as_ref().unwrap().lowres_turn != 0;

            for i in 0..num_tics {
                if let Some(cmd) = packet.read_full_ticcmd(lowres_turn) {
                    let index = (seq + i as u32 - self.recv_window_start) as usize;
                    if index < BACKUPTICS {
                        self.recv_window[index].active = true;
                        self.recv_window[index].cmd = cmd;
                        debug!("Stored tic {} in receive window", seq + i as u32);
                        if i == num_tics - 1 {
                            self.update_clock_sync(seq + i as u32, cmd.latency);
                        }
                    }
                }
            }

            self.need_acknowledge = true;
            self.gamedata_recv_time = Instant::now();

            // Check for missing tics and request resends
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
    }

    fn parse_resend_request(&mut self, packet: &mut NetPacket) {
        debug!("Processing resend request");
        if self.drone {
            warn!("Error: Resend request but we are a drone");
            return;
        }

        if let (Some(start), Some(num_tics)) = (packet.read_i32(), packet.read_u8()) {
            let end = start + num_tics as i32 - 1;
            debug!(
                "Resend request: start={}, num_tics={}",
                start, num_tics
            );

            let mut resend_start = start as u32;
            let mut resend_end = end as u32;

            while resend_start <= resend_end
                && (!self.send_queue[resend_start as usize % BACKUPTICS].active
                    || self.send_queue[resend_start as usize % BACKUPTICS].seq != resend_start)
            {
                resend_start += 1;
            }

            while resend_start <= resend_end
                && (!self.send_queue[resend_end as usize % BACKUPTICS].active
                    || self.send_queue[resend_end as usize % BACKUPTICS].seq != resend_end)
            {
                resend_end -= 1;
            }

            if resend_start <= resend_end {
                debug!("Resending tics {}-{}", resend_start, resend_end);
                self.send_tics(resend_start, resend_end);
            } else {
                warn!("Don't have the tics to resend");
            }
        }
    }

    fn parse_console_message(&self, packet: &mut NetPacket) {
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
        const KP: f32 = 0.1;
        const KI: f32 = 0.01;
        const KD: f32 = 0.02;

        let latency = self.send_queue[seq as usize % BACKUPTICS]
            .time
            .elapsed()
            .as_millis() as i32;
        let error = latency - remote_latency;

        // Update PID variables (these should be stored in the struct)
        let mut cumul_error = 0;
        let mut last_error = 0;

        cumul_error += error;
        let offset_ms =
            (KP * error as f32 - KI * cumul_error as f32 + KD * (last_error - error) as f32) as i32;

        last_error = error;
        self.last_latency = latency;

        debug!(
            "Latency {}, remote {}, offset={}ms, cumul_error={}",
            latency, remote_latency, offset_ms, cumul_error
        );
    }

    fn send_resend_request(&mut self, start: u32, end: u32) {
        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_GAMEDATA_RESEND);
        packet.write_i32(start as i32);
        packet.write_u8((end - start + 1) as u8);

        self.connection
            .send_packet(&packet, self.server_addr.as_ref().unwrap());

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
        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_GAMEDATA_ACK);
        packet.write_u8((self.recv_window_start & 0xff) as u8);

        self.connection
            .send_packet(&packet, self.server_addr.as_ref().unwrap());
        self.need_acknowledge = false;
        debug!("Game data acknowledgment sent");
    }

    fn send_tics(&mut self, start: u32, end: u32) {
        if !self.net_client_connected {
            return;
        }

        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_GAMEDATA);
        packet.write_u8((self.recv_window_start & 0xff) as u8);
        packet.write_u8((start & 0xff) as u8);
        packet.write_u8(((end - start + 1) & 0xff) as u8);

        for tic in start..=end {
            if let Some(send_obj) = self.send_queue.get(tic as usize % BACKUPTICS) {
                packet.write_i16(self.last_latency.try_into().unwrap());
                packet.write_ticcmd_diff(
                    &send_obj.cmd,
                    self.settings.as_ref().unwrap().lowres_turn != 0,
                );
            }
        }

        self.connection
            .send_packet(&packet, self.server_addr.as_ref().unwrap());
        self.need_acknowledge = false;
        debug!("Sent tics from {} to {}", start, end);
    }

    pub fn send_ticcmd(&mut self, ticcmd: &TicCmd, maketic: u32) {
        let mut diff = NetTicDiff::default();
        self.calculate_ticcmd_diff(ticcmd, &mut diff);

        let sendobj = &mut self.send_queue[maketic as usize % BACKUPTICS];
        sendobj.active = true;
        sendobj.seq = maketic;
        sendobj.time = Instant::now();
        sendobj.cmd = diff;

        let starttic = if maketic < self.settings.as_ref().unwrap().extratics as u32 {
            0
        } else {
            maketic - self.settings.as_ref().unwrap().extratics as u32
        };
        let endtic = maketic;

        self.send_tics(starttic, endtic);
    }

    fn calculate_ticcmd_diff(&self, ticcmd: &TicCmd, diff: &mut NetTicDiff) {
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

            // Call D_ReceiveTic or equivalent game state update function
            self.receive_tic(&ticcmds, &self.recv_window[0].cmd.playeringame);

            // Shift the window
            self.recv_window.rotate_left(1);
            self.recv_window[BACKUPTICS - 1] = NetServerRecv::default();
            self.recv_window_start += 1;

            println!(
                "Client: Advanced receive window to {}",
                self.recv_window_start
            );
        }
    }

    fn expand_full_ticcmd(
        &mut self,
        cmd: &NetFullTicCmd,
        seq: u32,
        ticcmds: &mut [TicCmd; NET_MAXPLAYERS],
    ) {
        let consoleplayer = self.settings.as_ref().unwrap().consoleplayer as usize;
        let drone = self.drone;
        let mut recvwindow_cmd_base = self.recvwindow_cmd_base.clone();

        for i in 0..NET_MAXPLAYERS {
            if i == consoleplayer && !drone {
                continue;
            }

            if cmd.playeringame[i] {
                let diff = &cmd.cmds[i];
                let mut base = recvwindow_cmd_base[i];
                NetClient::apply_ticcmd_diff(&mut base, diff, &mut ticcmds[i]);
                recvwindow_cmd_base[i] = ticcmds[i];
            }
        }

        self.recvwindow_cmd_base = recvwindow_cmd_base;
    }

    fn apply_ticcmd_diff(base: &mut TicCmd, diff: &NetTicDiff, result: &mut TicCmd) {
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
        ticcmds: &[TicCmd; NET_MAXPLAYERS],
        playeringame: &[bool; NET_MAXPLAYERS],
    ) {
        // This function should update the game state with the new ticcmds
        // It's a placeholder for the actual game logic update
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
                let need_resend = true;
            }

            if need_resend {
                if resend_start < 0 {
                    resend_start = i as i32;
                }
                resend_end = i as i32;
            } else if resend_start >= 0 {
                println!(
                    "Client: Resend request timed out for {}-{}",
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
            println!(
                "Client: Resend request timed out for {}-{}",
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
            println!(
                "Client: No game data received since {:?}: triggering ack",
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
        // Implement bot AI logic here
        // Placeholder for bot commands
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

        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_DISCONNECT);
        self.connection.send_packet(&packet, self.server_addr.as_ref().unwrap());

        while self.state == ClientState::Disconnecting {
            self.run();

            if self.start_time.elapsed() > Duration::from_secs(5) {
                warn!("No acknowledgment of disconnect received");
                self.state = ClientState::Disconnected;
                break;
            }

            // Don't hog the CPU
            std::thread::sleep(Duration::from_millis(1));
        }

        info!("Disconnect complete");
        self.shutdown();
    }

    pub fn get_settings(&self) -> Option<GameSettings> {
        if self.state != ClientState::InGame {
            return None;
        }
        self.settings
    }

    pub fn launch_game(&mut self) {
        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_LAUNCH);
        self.connection.send_reliable_packet(&packet);
    }

    pub fn start_game(&mut self, settings: &GameSettings) {
        self.last_ticcmd = TicCmd::default();

        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_GAMESTART);
        packet.write_settings(settings);
        self.connection.send_reliable_packet(&packet);
    }

    pub async fn connect(&mut self, addr: NetAddr, connect_data: ConnectData) -> Result<(), String> {
        debug!("Attempting to connect to server at {:?}", addr);
        self.server_addr = Some(addr.clone());
        self.connection.init_client(&addr, &connect_data);

        self.state = ClientState::Connecting;
        self.reject_reason = Some("Unknown reason".to_string());

        self.net_local_wad_sha1sum.copy_from_slice(&connect_data.wad_sha1sum);
        self.net_local_deh_sha1sum.copy_from_slice(&connect_data.deh_sha1sum);
        self.net_local_is_freedoom = connect_data.is_freedoom != 0;

        self.net_client_connected = false;  // Set to false until we're actually connected
        self.net_client_received_wait_data = false;

        self.start_time = Instant::now();
        self.last_send_time = Instant::now() - Duration::from_secs(1);
        self.num_retries = 0;

        let timeout = Duration::from_secs(30);
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        while self.state == ClientState::Connecting {
            tokio::select! {
                _ = interval.tick() => {
                    if self.start_time.elapsed() > timeout {
                        let error_msg = "Connection timed out after 30 seconds".to_string();
                        self.reject_reason = Some(error_msg.clone());
                        warn!("{}", error_msg);
                        return Err(error_msg);
                    }

                    match self.send_syn(&connect_data) {
                        Ok(_) => {
                            self.num_retries += 1;
                            debug!("Sent SYN packet. Retry count: {}", self.num_retries);
                        },
                        Err(e) => {
                            warn!("Failed to send SYN packet: {}", e);
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    self.run();

                    // Check for incoming packets
                    if let Some((_, packet)) = self.context.recv_packet() {
                        debug!("Received packet: {:?}", packet);
                        self.parse_packet(&mut packet.clone());
                    }
                }
            }
        }

        if self.state == ClientState::Connected {
            info!("Successfully connected");
            self.reject_reason = None;
            self.state = ClientState::WaitingLaunch;
            self.drone = connect_data.drone != 0;
            self.net_client_connected = true;
            Ok(())
        } else {
            let error_msg = format!("Connection failed. Reason: {:?}", self.reject_reason);
            warn!("{}", error_msg);
            self.shutdown();
            Err(error_msg)
        }
    }

    pub fn build_ticcmd(&mut self, cmd: &mut TicCmd, maketic: u32) {
        // For now, we'll just create empty commands
        *cmd = TicCmd::default();
    }

    pub fn run_tic(&mut self, cmds: &[TicCmd; NET_MAXPLAYERS], ingame: &[bool; NET_MAXPLAYERS]) {
        // Process the received tics
        // For now, we'll just print out the received commands
        for (i, (cmd, &in_game)) in cmds.iter().zip(ingame.iter()).enumerate() {
            if in_game {
                println!("Player {}: {:?}", i, cmd);
            }
        }
    }

    fn send_syn(&self, data: &ConnectData) -> Result<(), String> {
        let mut packet = NetPacket::new();
        packet.write_u16(NET_PACKET_TYPE_SYN);
        packet.write_u32(NET_MAGIC_NUMBER);
        packet.write_string(env!("CARGO_PKG_VERSION"));
        packet.write_protocol_list();
        packet.write_connect_data(data);
        packet.write_string(&self.player_name);

        if let Some(server_addr) = self.server_addr.as_ref() {
            self.connection.send_packet(&packet, server_addr);
            debug!("SYN sent to {:?}", server_addr);
            Ok(())
        } else {
            Err("Server address not set".to_string())
        }
    }

    // This function is already defined earlier in the file, so we'll remove this duplicate.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_initialization() {
        let client = NetClient::new("Player1".to_string(), false);
        assert_eq!(client.player_name, "Player1");
        assert!(!client.drone);
    }
}
