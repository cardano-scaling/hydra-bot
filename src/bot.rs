use crate::net::{GameSettings, TicCmd, NET_MAXPLAYERS};
use rand::Rng;
use std::cmp::{max, min};

const FORWARDWALK: i8 = 25;
const FORWARDRUN: i8 = 50;
const SIDEWALK: i8 = 24;
const SIDERUN: i8 = 40;

const AFTERTICS: i32 = 70;
const MAXROAM: i32 = 140;

pub struct Bot {
    pub settings: Option<GameSettings>,
    pub recvwindow_cmd_base: [TicCmd; NET_MAXPLAYERS],
    enemy: Option<usize>,
    dest: Option<usize>,
    prev: Option<usize>,
    mate: Option<usize>,
    t_active: i32,
    t_respawn: i32,
    t_strafe: i32,
    t_react: i32,
    t_fight: i32,
    t_roam: i32,
    t_rocket: i32,
    first_shot: bool,
    sleft: bool,
    angle: i32,
    oldx: i32,
    oldy: i32,
    skill: BotSkill,
    allround: bool,
    increase: bool,
}

struct BotSkill {
    aiming: i32,
    perfection: i32,
    reaction: i32,
    isp: i32,
}

impl Bot {
    pub fn new(settings: Option<GameSettings>) -> Self {
        Self {
            settings,
            recvwindow_cmd_base: [TicCmd::default(); NET_MAXPLAYERS],
            enemy: None,
            dest: None,
            prev: None,
            mate: None,
            t_active: 0,
            t_respawn: 0,
            t_strafe: 0,
            t_react: 0,
            t_fight: 0,
            t_roam: 0,
            t_rocket: 0,
            first_shot: true,
            sleft: false,
            angle: 0,
            oldx: 0,
            oldy: 0,
            skill: BotSkill {
                aiming: 50,
                perfection: 50,
                reaction: 50,
                isp: 50,
            },
            allround: false,
            increase: false,
        }
    }

    pub fn init(&mut self) {
        self.t_active = 0;
        self.t_respawn = 0;
        self.t_strafe = 0;
        self.t_react = 0;
        self.t_fight = 0;
        self.t_roam = 0;
        self.t_rocket = 0;
        self.first_shot = true;
        self.allround = false;
    }

    pub fn build_ticcmd(&mut self, ticcmd: &mut TicCmd, maketic: u32) {
        if let Some(settings) = &self.settings {
            if settings.consoleplayer >= 0 {
                self.think(ticcmd, maketic);
            }
        }
    }

    fn think(&mut self, cmd: &mut TicCmd, maketic: u32) {
        self.set_enemy();
        self.think_for_move(cmd);
        self.turn_to_angle();

        // Update cmd based on angle
        cmd.angleturn = self.angle as i16;

        // Decrease timers
        self.t_active = max(0, self.t_active - 1);
        self.t_strafe = max(0, self.t_strafe - 1);
        self.t_react = max(0, self.t_react - 1);
        self.t_fight = max(0, self.t_fight - 1);
        self.t_rocket = max(0, self.t_rocket - 1);
        self.t_roam = max(0, self.t_roam - 1);

        // Handle respawn
        if self.t_respawn > 0 {
            self.t_respawn -= 1;
        } else if maketic % 35 == 0 {
            // Check every second (35 tics)
            cmd.buttons |= 1; // BT_USE
        }

        // Update position
        self.oldx = cmd.forwardmove as i32;
        self.oldy = cmd.sidemove as i32;
    }

    fn set_enemy(&mut self) {
        if self.enemy.is_none() || self.t_fight <= 0 {
            // Simplified enemy selection logic
            let mut rng = rand::thread_rng();
            if rng.gen_bool(0.7) {
                // 70% chance to choose a new enemy
                self.enemy = Some(rng.gen_range(0..NET_MAXPLAYERS));
                self.t_fight = AFTERTICS;
            } else {
                self.enemy = None;
            }
        }
    }

    fn think_for_move(&mut self, cmd: &mut TicCmd) {
        if self.enemy.is_some() {
            self.combat_movement(cmd);
        } else if self.mate.is_some() {
            self.follow_mate(cmd);
        } else {
            self.roam(cmd);
        }
    }

    fn combat_movement(&mut self, cmd: &mut TicCmd) {
        if self.t_strafe <= 0 {
            self.t_strafe = 5;
            self.sleft = !self.sleft;
        }

        cmd.forwardmove = FORWARDRUN;
        cmd.sidemove = if self.sleft { -SIDERUN } else { SIDERUN };

        self.do_fire(cmd);
    }

    fn follow_mate(&mut self, cmd: &mut TicCmd) {
        // Simplified mate following logic
        cmd.forwardmove = FORWARDWALK;
        cmd.sidemove = if self.sleft { -SIDEWALK } else { SIDEWALK };
    }

    fn roam(&mut self, cmd: &mut TicCmd) {
        if self.t_roam <= 0 {
            self.choose_destination();
            self.t_roam = MAXROAM;
        }

        if self.dest.is_some() {
            self.move_to_destination(cmd);
        } else {
            cmd.forwardmove = FORWARDWALK;
            if rand::thread_rng().gen_bool(0.1) {
                self.sleft = !self.sleft;
            }
            cmd.sidemove = if self.sleft { -SIDEWALK } else { SIDEWALK };
        }
    }

    fn choose_destination(&mut self) {
        // Simplified destination selection
        self.prev = self.dest;
        self.dest = Some(rand::thread_rng().gen_range(0..NET_MAXPLAYERS));
    }

    fn move_to_destination(&mut self, cmd: &mut TicCmd) {
        // Simplified movement towards destination
        cmd.forwardmove = FORWARDRUN;
        if rand::thread_rng().gen_bool(0.1) {
            self.sleft = !self.sleft;
        }
        cmd.sidemove = if self.sleft { -SIDERUN } else { SIDERUN };
    }

    fn turn_to_angle(&mut self) {
        let target_angle = if self.enemy.is_some() {
            // Aim at enemy
            rand::thread_rng().gen_range(0..360)
        } else if self.dest.is_some() {
            // Turn towards destination
            rand::thread_rng().gen_range(0..360)
        } else {
            // Random wandering
            self.angle + rand::thread_rng().gen_range(-5..6)
        };

        // Adjust current angle towards target angle
        let diff = (target_angle - self.angle + 180) % 360 - 180;
        self.angle += diff.signum() * min(diff.abs(), 15);
        self.angle = (self.angle + 360) % 360;

        // Update allround flag
        self.allround = self.enemy.is_some() || self.mate.is_some();
    }

    fn do_fire(&mut self, cmd: &mut TicCmd) {
        if self.t_react == 0 {
            cmd.buttons |= 2; // BT_ATTACK
            self.first_shot = false;
        } else if self.first_shot {
            self.t_react = (100 - self.skill.reaction) / 3;
            self.first_shot = false;
        }

        // Update increase flag for smoother turning
        self.increase = !self.increase;
    }
}
