use crate::net::client::Client;
use crate::net::{TicCmd, TicDiff, BACKUPTICS, NET_MAXPLAYERS};

use std::time::{Duration, Instant};
use tracing::{debug, warn};

pub const TICRATE: u32 = 35;
const MAX_NETGAME_STALL_TICS: u32 = 2;

#[derive(Clone, Copy)]
struct TiccmdSet {
    cmds: [TicCmd; NET_MAXPLAYERS],
    ingame: [bool; NET_MAXPLAYERS],
}

pub struct Game {
    ticdata: [TiccmdSet; BACKUPTICS],
    maketic: i32,
    recvtic: i32,
    gametic: i32,
    localplayer: i32,
    offsetms: i32,
    ticdup: i32,
    new_sync: bool,
    local_playeringame: [bool; NET_MAXPLAYERS],
    frameskip: [bool; 4],
    singletics: bool,
    lasttime: i32,
    skiptics: i32,
    frameon: i32,
    oldnettics: i32,
    oldentertics: i32,
    last_net_update: Instant,
    settings: Option<crate::net::GameSettings>,
    bot: crate::bot::Bot,
    drone: bool,
    send_queue: [crate::net::ServerSend; BACKUPTICS],
    last_ticcmd: TicCmd,
}

impl Game {
    pub fn new() -> Self {
        Game {
            ticdata: [TiccmdSet {
                cmds: [TicCmd::default(); NET_MAXPLAYERS],
                ingame: [false; NET_MAXPLAYERS],
            }; BACKUPTICS],
            maketic: 0,
            recvtic: 0,
            gametic: 0,
            localplayer: 0,
            offsetms: 0,
            ticdup: 1,
            new_sync: true,
            local_playeringame: [false; NET_MAXPLAYERS],
            frameskip: [false; 4],
            singletics: false,
            lasttime: 0,
            skiptics: 0,
            frameon: 0,
            oldnettics: 0,
            oldentertics: 0,
            last_net_update: Instant::now(),
            settings: None,
            bot: crate::bot::Bot::new(None),
            drone: false,
            send_queue: [crate::net::ServerSend::default(); BACKUPTICS],
            last_ticcmd: TicCmd::default(),
        }
    }

    fn get_adjusted_time(&self) -> u32 {
        let time_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i32;

        if self.new_sync {
            (time_ms + self.offsetms) as u32 * TICRATE / 1000
        } else {
            time_ms as u32 * TICRATE / 1000
        }
    }

    pub fn build_and_send_tic(&mut self, client: &mut Client) -> bool {
        let maketic = self.maketic as u32;
        if maketic % self.settings.as_ref().map_or(1, |s| s.ticdup as u32) != 0 {
            return true;
        }

        let mut ticcmd = TicCmd::default();
        self.bot.build_ticcmd(&mut ticcmd, maketic);

        let mut diff = TicDiff::default();
        client.calculate_ticcmd_diff(&ticcmd, &mut diff);

        if !self.drone {
            let sendobj = &mut self.send_queue[maketic as usize % BACKUPTICS];
            sendobj.active = true;
            sendobj.seq = maketic;
            sendobj.time = Instant::now();
            sendobj.cmd = diff;

            let starttic =
                maketic.saturating_sub(self.settings.as_ref().map_or(0, |s| s.extratics as u32));
            let endtic = maketic;

            client.send_tics(starttic, endtic);
        }

        self.last_ticcmd = ticcmd;
        self.maketic += 1;
        true
    }

    pub fn net_update(&mut self, client: &mut Client) {
        if self.singletics {
            return;
        }

        let now = Instant::now();
        if now.duration_since(self.last_net_update) < Duration::from_millis(1000 / TICRATE as u64) {
            return;
        }
        self.last_net_update = now;

        let nowtime = self.get_adjusted_time();
        self.lasttime = nowtime as i32;

        client.run();

        let nowtime = (self.get_adjusted_time() / self.ticdup as u32) as i32;
        let mut newtics = nowtime.saturating_sub(self.lasttime) as u32;

        self.lasttime = nowtime;

        if self.skiptics <= newtics as i32 {
            newtics = newtics.saturating_sub(self.skiptics as u32);
            self.skiptics = 0;
        } else {
            self.skiptics -= newtics as i32;
            newtics = 0;
        }

        for _ in 0..newtics {
            client.build_and_send_tic(self.maketic as u32);
        }
    }

    pub fn start_loop(&mut self) {
        self.lasttime = (self.get_adjusted_time() / self.ticdup as u32) as i32;
    }

    pub fn tick(&mut self, client: &mut Client) {
        self.last_net_update = Instant::now();

        let enter_tic = (self.get_adjusted_time() / self.ticdup as u32) as i32;

        let mut counts;
        let mut lowtic;

        if self.singletics {
            client.build_and_send_tic(self.maketic as u32);
        } else {
            self.net_update(client);
        }

        lowtic = self.get_low_tic();

        let availabletics = lowtic - self.gametic / self.ticdup;

        let realtics = enter_tic - self.oldentertics;
        self.oldentertics = enter_tic;

        if self.new_sync {
            counts = availabletics;
        } else {
            counts = realtics.min(availabletics).max(1);

            if client.is_connected() {
                self.old_net_sync();
            }
        }

        while !self.players_in_game(client) || lowtic < self.gametic / self.ticdup + counts {
            client.run();

            lowtic = self.get_low_tic();

            if lowtic < self.gametic / self.ticdup {
                panic!("TryRunTics: lowtic < gametic");
            }

            if lowtic < self.gametic / self.ticdup + counts {
                if self.get_adjusted_time() / self.ticdup as u32 - enter_tic as u32
                    >= MAX_NETGAME_STALL_TICS
                {
                    warn!("work stall detected");
                    return;
                }

                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        while counts > 0 {
            if !self.players_in_game(client) {
                return;
            }

            let set = &mut self.ticdata[(self.gametic / self.ticdup) as usize % BACKUPTICS];

            if !client.is_connected() {
                Self::single_player_clear(self.localplayer as usize, set);
            }

            for _ in 0..self.ticdup {
                if self.gametic / self.ticdup > lowtic {
                    panic!("gametic>lowtic");
                }

                self.local_playeringame = set.ingame;

                client.run_tic(&set.cmds, &set.ingame);
                self.gametic += 1;

                Self::ticdup_squash(set);
            }

            if !self.build_and_send_tic(client) {
                break;
            }

            client.run();
            counts -= 1;
        }
        debug!("Finished running tics. New gametic: {}", self.gametic);
    }

    fn get_low_tic(&self) -> i32 {
        self.maketic.min(self.recvtic)
    }

    fn old_net_sync(&mut self) {
        self.frameon += 1;

        let keyplayer = self.local_playeringame.iter().position(|&x| x).unwrap_or(0) as i32;

        if self.localplayer != keyplayer {
            if self.maketic <= self.recvtic {
                self.lasttime -= 1;
            }

            let frameon = self.frameon as usize;
            self.frameskip[frameon & 3] = self.oldnettics > self.recvtic;
            self.oldnettics = self.maketic;

            if self.frameskip.iter().all(|&x| x) {
                self.skiptics = 1;
            }
        }
    }

    fn players_in_game(&self, client: &Client) -> bool {
        if client.is_connected() {
            self.local_playeringame.iter().any(|&x| x)
        } else {
            !client.is_drone()
        }
    }

    fn single_player_clear(localplayer: usize, set: &mut TiccmdSet) {
        for i in 0..NET_MAXPLAYERS {
            if i != localplayer {
                set.ingame[i] = false;
            }
        }
    }

    fn ticdup_squash(set: &mut TiccmdSet) {
        for cmd in &mut set.cmds {
            cmd.chatchar = 0;
            if cmd.buttons & 0x80 != 0 {
                // 0x80 is the value for BT_SPECIAL
                cmd.buttons = 0;
            }
        }
    }
}
