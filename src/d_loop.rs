use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::net_structs::{NET_MAXPLAYERS, BACKUPTICS, TicCmd, GameSettings};
use crate::net_client;

// Constants
const TICRATE: u32 = 35;
const MAX_NETGAME_STALL_TICS: u32 = 2;

// Structs
#[derive(Clone, Copy)]
struct TiccmdSet {
    cmds: [TicCmd; NET_MAXPLAYERS],
    ingame: [bool; NET_MAXPLAYERS],
}

// Global variables
static INSTANCE_UID: AtomicU32 = AtomicU32::new(0);
static mut TICDATA: [TiccmdSet; BACKUPTICS] = [TiccmdSet {
    cmds: [TicCmd::default(); NET_MAXPLAYERS],
    ingame: [false; NET_MAXPLAYERS],
}; BACKUPTICS];

static mut MAKETIC: i32 = 0;
static mut RECVTIC: i32 = 0;
static mut GAMETIC: i32 = 0;
static mut LOCALPLAYER: i32 = 0;
static mut OFFSETMS: i32 = 0;
static mut TICDUP: i32 = 1;
static mut NEW_SYNC: bool = true;
static mut LOCAL_PLAYERINGAME: [bool; NET_MAXPLAYERS] = [false; NET_MAXPLAYERS];

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
    let gameticdiv = MAKETIC.load(Ordering::Relaxed) / TICDUP;

    // Call ProcessEvents from loop_interface
    process_events();

    // Always run the menu
    run_menu();

    if DRONE.load(Ordering::Relaxed) {
        // In drone mode, do not generate any ticcmds.
        return false;
    }

    if unsafe { NEW_SYNC } {
        // If playing single player, do not allow tics to buffer up very far
        if !net_client::is_connected() && unsafe { MAKETIC - gameticdiv > 2 } {
            return false;
        }

        // Never go more than ~200ms ahead
        if unsafe { MAKETIC - gameticdiv > 8 } {
            return false;
        }
    } else {
        if unsafe { MAKETIC - gameticdiv >= 5 } {
            return false;
        }
    }

    let mut cmd = TicCmd::default();
    unsafe { loop_interface.build_ticcmd(&mut cmd, MAKETIC) };

    if net_client::is_connected() {
        net_client::send_ticcmd(&cmd, unsafe { MAKETIC });
    }

    unsafe {
        TICDATA[MAKETIC % BACKUPTICS].cmds[LOCALPLAYER as usize] = cmd;
        TICDATA[MAKETIC % BACKUPTICS].ingame[LOCALPLAYER as usize] = true;
        MAKETIC += 1;
    }

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
    let mut newtics = nowtime - LASTTIME.load(Ordering::Relaxed);
    LASTTIME.store(nowtime, Ordering::Relaxed);

    // Run network subsystems
    net_client::run();
    net_server::run();

    // check time
    nowtime = (get_adjusted_time() / unsafe { TICDUP } as u32) as i32;
    newtics = nowtime - unsafe { LASTTIME };

    unsafe { LASTTIME = nowtime };

    if unsafe { SKIPTICS <= newtics } {
        newtics -= unsafe { SKIPTICS };
        unsafe { SKIPTICS = 0 };
    } else {
        unsafe { SKIPTICS -= newtics };
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
    unsafe {
        LASTTIME = (get_adjusted_time() / TICDUP as u32) as i32;
    }
}

// TryRunTics function
pub fn try_run_tics() {
    let enter_tic = (get_adjusted_time() / unsafe { TICDUP } as u32) as i32;
    let mut realtics;
    let mut availabletics;
    let mut counts;
    let lowtic;

    if unsafe { SINGLETICS } {
        build_new_tic();
    } else {
        net_update();
    }

    lowtic = get_low_tic();

    availabletics = lowtic - unsafe { GAMETIC / TICDUP };

    realtics = enter_tic - unsafe { OLDENTERTICS };
    unsafe { OLDENTERTICS = enter_tic };

    if unsafe { NEW_SYNC } {
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
    while !players_in_game() || lowtic < unsafe { GAMETIC / TICDUP + counts } {
        net_update();

        lowtic = get_low_tic();

        if lowtic < unsafe { GAMETIC / TICDUP } {
            panic!("TryRunTics: lowtic < gametic");
        }

        // Still no tics to run? Sleep until some are available.
        if lowtic < unsafe { GAMETIC / TICDUP + counts } {
            // If we're in a netgame, we might spin forever waiting for
            // new network data to be received. So don't stay in here
            // forever - give the menu a chance to work.
            if get_adjusted_time() / unsafe { TICDUP } as u32 - enter_tic as u32 >= MAX_NETGAME_STALL_TICS {
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
            let set = &mut TICDATA[(GAMETIC / TICDUP) as usize % BACKUPTICS];

            if !net_client::is_connected() {
                single_player_clear(set);
            }

            for _ in 0..TICDUP {
                if GAMETIC / TICDUP > lowtic {
                    panic!("gametic>lowtic");
                }

                LOCAL_PLAYERINGAME.copy_from_slice(&set.ingame);

                loop_interface.run_tic(&set.cmds, &set.ingame);
                GAMETIC += 1;

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
    unsafe {
        FRAMEON += 1;

        let keyplayer = LOCAL_PLAYERINGAME.iter().position(|&x| x).unwrap_or(0) as i32;

        if LOCALPLAYER != keyplayer {
            if MAKETIC <= RECVTIC {
                LASTTIME -= 1;
            }

            FRAMESKIP[FRAMEON as usize & 3] = OLDNETTICS > RECVTIC;
            OLDNETTICS = MAKETIC;

            if FRAMESKIP.iter().all(|&x| x) {
                SKIPTICS = 1;
            }
        }
    }
}

fn players_in_game() -> bool {
    if net_client::is_connected() {
        // Assuming LOCAL_PLAYERINGAME is a global array of booleans
        LOCAL_PLAYERINGAME.iter().any(|&x| x)
    } else {
        !DRONE.load(Ordering::Relaxed)
    }
}

fn single_player_clear(set: &mut TiccmdSet) {
    for i in 0..NET_MAXPLAYERS {
        if i != unsafe { LOCALPLAYER } as usize {
            set.ingame[i] = false;
        }
    }
}

fn ticdup_squash(set: &mut TiccmdSet) {
    for cmd in &mut set.cmds {
        cmd.chatchar = 0;
        if cmd.buttons & BT_SPECIAL != 0 {
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
