use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU32, AtomicI32, AtomicBool, Ordering};

use crate::net_structs::{TicCmd, GameSettings};
use crate::net_client;

// Constants
const TICRATE: u32 = 35;
const MAX_NETGAME_STALL_TICS: u32 = 2;

// Structs
#[derive(Clone, Copy)]
struct TiccmdSet {
    cmds: [TicCmd; crate::net_structs::NET_MAXPLAYERS],
    ingame: [bool; crate::net_structs::NET_MAXPLAYERS],
}

// Global variables
static INSTANCE_UID: AtomicU32 = AtomicU32::new(0);
static mut TICDATA: [TiccmdSet; crate::net_structs::BACKUPTICS] = [TiccmdSet {
    cmds: [TicCmd::default(); crate::net_structs::NET_MAXPLAYERS],
    ingame: [false; crate::net_structs::NET_MAXPLAYERS],
}; crate::net_structs::BACKUPTICS];

static MAKETIC: AtomicI32 = AtomicI32::new(0);
static RECVTIC: AtomicI32 = AtomicI32::new(0);
static GAMETIC: AtomicI32 = AtomicI32::new(0);
static LOCALPLAYER: AtomicI32 = AtomicI32::new(0);
static OFFSETMS: AtomicI32 = AtomicI32::new(0);
static TICDUP: AtomicI32 = AtomicI32::new(1);
static NEW_SYNC: AtomicBool = AtomicBool::new(true);
static mut LOCAL_PLAYERINGAME: [bool; crate::net_structs::NET_MAXPLAYERS] = [false; crate::net_structs::NET_MAXPLAYERS];

// Function to get adjusted time
fn get_adjusted_time() -> u32 {
    let time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i32;

    if NEW_SYNC.load(Ordering::Relaxed) {
        (time_ms + OFFSETMS.load(Ordering::Relaxed)) as u32 * TICRATE / 1000
    } else {
        time_ms as u32 * TICRATE / 1000
    }
}

// Function to build new tic
fn build_new_tic() -> bool {
    let gameticdiv = MAKETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed);

    // Call ProcessEvents from loop_interface
    // process_events();

    // Always run the menu
    // run_menu();

    if DRONE.load(Ordering::Relaxed) {
        // In drone mode, do not generate any ticcmds.
        return false;
    }

    if NEW_SYNC.load(Ordering::Relaxed) {
        // If playing single player, do not allow tics to buffer up very far
        if !net_client::is_connected() && MAKETIC.load(Ordering::Relaxed) - gameticdiv > 2 {
            return false;
        }

        // Never go more than ~200ms ahead
        if MAKETIC.load(Ordering::Relaxed) - gameticdiv > 8 {
            return false;
        }
    } else {
        if MAKETIC.load(Ordering::Relaxed) - gameticdiv >= 5 {
            return false;
        }
    }

    let mut cmd = TicCmd::default();
    // unsafe { loop_interface.build_ticcmd(&mut cmd, MAKETIC.load(Ordering::Relaxed)) };

    if net_client::is_connected() {
        net_client::send_ticcmd(&cmd, MAKETIC.load(Ordering::Relaxed));
    }

    unsafe {
        let maketic = MAKETIC.load(Ordering::Relaxed) as usize;
        let localplayer = LOCALPLAYER.load(Ordering::Relaxed) as usize;
        TICDATA[maketic % crate::net_structs::BACKUPTICS].cmds[localplayer] = cmd;
        TICDATA[maketic % crate::net_structs::BACKUPTICS].ingame[localplayer] = true;
    }
    MAKETIC.fetch_add(1, Ordering::Relaxed);

    true
}

// NetUpdate function
pub fn net_update() {
    // If we are running with singletics (timing a demo), this
    // is all done separately.
    if SINGLETICS.load(Ordering::Relaxed) {
        return;
    }

    let nowtime = get_adjusted_time();
    let mut newtics = nowtime.saturating_sub(LASTTIME.load(Ordering::Relaxed) as u32);
    LASTTIME.store(nowtime as i32, Ordering::Relaxed);

    // Run network subsystems
    net_client::run();
    // net_server::run();

    // check time
    let nowtime = (get_adjusted_time() / TICDUP.load(Ordering::Relaxed) as u32) as i32;
    newtics = nowtime.saturating_sub(LASTTIME.load(Ordering::Relaxed)) as u32;

    LASTTIME.store(nowtime, Ordering::Relaxed);

    let mut skiptics = SKIPTICS.load(Ordering::Relaxed);
    if skiptics <= newtics as i32 {
        newtics = newtics.saturating_sub(skiptics as u32);
        SKIPTICS.store(0, Ordering::Relaxed);
    } else {
        SKIPTICS.fetch_sub(newtics as i32, Ordering::Relaxed);
        newtics = 0;
    }

    // build new ticcmds for console player
    for _ in 0..newtics {
        if !build_new_tic() {
            break;
        }
    }
}

// D_StartGameLoop function
pub fn d_start_game_loop() {
    LASTTIME.store((get_adjusted_time() / TICDUP.load(Ordering::Relaxed) as u32) as i32, Ordering::Relaxed);
}

// TryRunTics function
pub fn try_run_tics() {
    let enter_tic = (get_adjusted_time() / TICDUP.load(Ordering::Relaxed) as u32) as i32;
    let mut realtics;
    let mut availabletics;
    let mut counts;
    let lowtic;

    if SINGLETICS.load(Ordering::Relaxed) {
        build_new_tic();
    } else {
        net_update();
    }

    lowtic = get_low_tic();

    availabletics = lowtic - GAMETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed);

    realtics = enter_tic - OLDENTERTICS.load(Ordering::Relaxed);
    OLDENTERTICS.store(enter_tic, Ordering::Relaxed);

    if NEW_SYNC.load(Ordering::Relaxed) {
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

        if net_client::is_connected() {
            old_net_sync();
        }
    }

    if counts < 1 {
        counts = 1;
    }

    // wait for new tics if needed
    while !players_in_game() || lowtic < GAMETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed) + counts {
        net_update();

        lowtic = get_low_tic();

        if lowtic < GAMETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed) {
            panic!("TryRunTics: lowtic < gametic");
        }

        // Still no tics to run? Sleep until some are available.
        if lowtic < GAMETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed) + counts {
            // If we're in a netgame, we might spin forever waiting for
            // new network data to be received. So don't stay in here
            // forever - give the menu a chance to work.
            if get_adjusted_time() / TICDUP.load(Ordering::Relaxed) as u32 - enter_tic as u32 >= MAX_NETGAME_STALL_TICS {
                return;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    while counts > 0 {
        if !players_in_game() {
            return;
        }

        unsafe {
            let set = &mut TICDATA[(GAMETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed)) as usize % crate::net_structs::BACKUPTICS];

            if !net_client::is_connected() {
                single_player_clear(set);
            }

            for _ in 0..TICDUP.load(Ordering::Relaxed) {
                if GAMETIC.load(Ordering::Relaxed) / TICDUP.load(Ordering::Relaxed) > lowtic {
                    panic!("gametic>lowtic");
                }

                LOCAL_PLAYERINGAME.copy_from_slice(&set.ingame);

                // loop_interface.run_tic(&set.cmds, &set.ingame);
                GAMETIC.fetch_add(1, Ordering::Relaxed);

                // modify command for duplicated tics
                ticdup_squash(set);
            }
        }

        net_update(); // check for new console commands
        counts -= 1;
    }
}

fn get_low_tic() -> i32 {
    let mut lowtic = MAKETIC.load(Ordering::Relaxed);

    if net_client::is_connected() {
        let recvtic = RECVTIC.load(Ordering::Relaxed);
        if DRONE.load(Ordering::Relaxed) || recvtic < lowtic {
            lowtic = recvtic;
        }
    }

    lowtic
}

fn old_net_sync() {
    FRAMEON.fetch_add(1, Ordering::Relaxed);

    let keyplayer = unsafe { LOCAL_PLAYERINGAME.iter().position(|&x| x).unwrap_or(0) as i32 };

    if LOCALPLAYER.load(Ordering::Relaxed) != keyplayer {
        if MAKETIC.load(Ordering::Relaxed) <= RECVTIC.load(Ordering::Relaxed) {
            LASTTIME.fetch_sub(1, Ordering::Relaxed);
        }

        let frameon = FRAMEON.load(Ordering::Relaxed) as usize;
        FRAMESKIP[frameon & 3] = OLDNETTICS.load(Ordering::Relaxed) > RECVTIC.load(Ordering::Relaxed);
        OLDNETTICS.store(MAKETIC.load(Ordering::Relaxed), Ordering::Relaxed);

        if FRAMESKIP.iter().all(|&x| x) {
            SKIPTICS.store(1, Ordering::Relaxed);
        }
    }
}

fn players_in_game() -> bool {
    if net_client::is_connected() {
        // Assuming LOCAL_PLAYERINGAME is a global array of booleans
        unsafe { LOCAL_PLAYERINGAME.iter().any(|&x| x) }
    } else {
        !DRONE.load(Ordering::Relaxed)
    }
}

fn single_player_clear(set: &mut TiccmdSet) {
    let localplayer = LOCALPLAYER.load(Ordering::Relaxed) as usize;
    for i in 0..crate::net_structs::NET_MAXPLAYERS {
        if i != localplayer {
            set.ingame[i] = false;
        }
    }
}

fn ticdup_squash(set: &mut TiccmdSet) {
    for cmd in &mut set.cmds {
        cmd.chatchar = 0;
        if cmd.buttons & crate::net_structs::BT_SPECIAL != 0 {
            cmd.buttons = 0;
        }
    }
}

// Initialize the module
pub fn init() {
    // Generate UID for this instance
    let uid = rand::random::<u32>() % 0xfffe;
    INSTANCE_UID.store(uid, Ordering::SeqCst);
    println!("doom: 8, uid is {}", uid);
}

static FRAMESKIP: [bool; 4] = [false; 4];
static DRONE: AtomicBool = AtomicBool::new(false);
static SINGLETICS: AtomicBool = AtomicBool::new(false);
static LASTTIME: AtomicI32 = AtomicI32::new(0);
static SKIPTICS: AtomicI32 = AtomicI32::new(0);
static FRAMEON: AtomicI32 = AtomicI32::new(0);
static OLDNETTICS: AtomicI32 = AtomicI32::new(0);
static OLDENTERTICS: AtomicI32 = AtomicI32::new(0);
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

const TICRATE: u32 = 35;
const NET_MAXPLAYERS: usize = 4;
const BACKUPTICS: usize = 128;

static OFFSETMS: AtomicI32 = AtomicI32::new(0);
static NEW_SYNC: AtomicBool = AtomicBool::new(false);
static DRONE: AtomicBool = AtomicBool::new(false);
static SINGLETICS: AtomicBool = AtomicBool::new(false);
static MAKETIC: AtomicI32 = AtomicI32::new(0);
static RECVTIC: AtomicI32 = AtomicI32::new(0);
static LASTTIME: AtomicI32 = AtomicI32::new(0);
static SKIPTICS: AtomicI32 = AtomicI32::new(0);
static FRAMEON: AtomicI32 = AtomicI32::new(0);
static OLDNETTICS: AtomicI32 = AtomicI32::new(0);
static LOCALPLAYER: AtomicI32 = AtomicI32::new(0);
