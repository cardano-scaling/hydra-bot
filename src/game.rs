use crate::net_client::NetClient;
use crate::net_structs::{GameSettings, TicCmd, BACKUPTICS, NET_MAXPLAYERS};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub const TICRATE: u32 = 35;
const MAX_NETGAME_STALL_TICS: u32 = 2;

#[derive(Clone, Copy)]
struct TiccmdSet {
    cmds: [TicCmd; NET_MAXPLAYERS],
    ingame: [bool; NET_MAXPLAYERS],
}

pub struct Game {
    instance_uid: u32,
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
    drone: bool,
    singletics: bool,
    lasttime: i32,
    skiptics: i32,
    frameon: i32,
    oldnettics: i32,
    oldentertics: i32,
}

impl Game {
    pub fn new() -> Self {
        let uid = rand::random::<u32>() % 0xfffe;
        debug!("doom: 8, uid is {}", uid);

        Game {
            instance_uid: uid,
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
            drone: false,
            singletics: false,
            lasttime: 0,
            skiptics: 0,
            frameon: 0,
            oldnettics: 0,
            oldentertics: 0,
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

    fn build_new_tic(&mut self, client: &mut NetClient) -> bool {
        let gameticdiv = self.maketic / self.ticdup;

        if client.is_drone() {
            return false;
        }

        if self.new_sync {
            if !client.is_connected() && self.maketic - gameticdiv > 2 {
                return false;
            }

            if self.maketic - gameticdiv > 8 {
                return false;
            }
        } else if self.maketic - gameticdiv >= 5 {
            return false;
        }

        let mut cmd = TicCmd::default();
        client.build_ticcmd(&mut cmd, self.maketic as u32);

        if client.is_connected() {
            client.send_ticcmd(&cmd, self.maketic as u32);
        }

        let maketic = self.maketic as usize;
        let localplayer = self.localplayer as usize;
        self.ticdata[maketic % BACKUPTICS].cmds[localplayer] = cmd;
        self.ticdata[maketic % BACKUPTICS].ingame[localplayer] = true;
        self.maketic += 1;

        true
    }

    pub fn net_update(&mut self, client: &mut NetClient) {
        if self.singletics {
            return;
        }

        let nowtime = self.get_adjusted_time();
        let mut newtics = nowtime.saturating_sub(self.lasttime as u32);
        self.lasttime = nowtime as i32;

        client.run();

        let nowtime = (self.get_adjusted_time() / self.ticdup as u32) as i32;
        newtics = nowtime.saturating_sub(self.lasttime) as u32;

        self.lasttime = nowtime;

        if self.skiptics <= newtics as i32 {
            newtics = newtics.saturating_sub(self.skiptics as u32);
            self.skiptics = 0;
        } else {
            self.skiptics -= newtics as i32;
            newtics = 0;
        }

        for _ in 0..newtics {
            if !self.build_new_tic(client) {
                break;
            }
        }
    }

    pub fn start_loop(&mut self) {
        self.lasttime = (self.get_adjusted_time() / self.ticdup as u32) as i32;
    }

    pub fn tick(&mut self, client: &mut NetClient) {
        let enter_tic = (self.get_adjusted_time() / self.ticdup as u32) as i32;
        let mut realtics;
        let mut availabletics;
        let mut counts;
        let mut lowtic;

        if self.singletics {
            self.build_new_tic(client);
        } else {
            self.net_update(client);
        }

        lowtic = self.get_low_tic();

        availabletics = lowtic - self.gametic / self.ticdup;

        realtics = enter_tic - self.oldentertics;
        self.oldentertics = enter_tic;

        if self.new_sync {
            counts = availabletics;
        } else {
            if realtics < availabletics - 1 {
                counts = realtics + 1;
            } else if realtics < availabletics {
                counts = realtics;
            } else {
                counts = availabletics;
            }

            if counts < 1 {
                counts = 1;
            }

            if client.is_connected() {
                self.old_net_sync();
            }
        }

        if counts < 1 {
            counts = 1;
        }

        while !self.players_in_game(client) || lowtic < self.gametic / self.ticdup + counts {
            self.net_update(client);

            lowtic = self.get_low_tic();

            if lowtic < self.gametic / self.ticdup {
                panic!("TryRunTics: lowtic < gametic");
            }

            if lowtic < self.gametic / self.ticdup + counts {
                if self.get_adjusted_time() / self.ticdup as u32 - enter_tic as u32
                    >= MAX_NETGAME_STALL_TICS
                {
                    warn!("Network stall detected");
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

            self.net_update(client);
            counts -= 1;
        }
        debug!("Finished running tics. New gametic: {}", self.gametic);
    }

    fn get_low_tic(&self) -> i32 {
        let mut lowtic = self.maketic;

        if self.recvtic < lowtic {
            lowtic = self.recvtic;
        }

        lowtic
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

    fn players_in_game(&self, client: &NetClient) -> bool {
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
