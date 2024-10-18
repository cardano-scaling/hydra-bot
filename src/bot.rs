use rand::prelude::*;

use crate::net::{GameSettings, TicCmd, NET_MAXPLAYERS};

pub struct Bot {
    rng: ThreadRng,
    settings: Option<GameSettings>,
    pub recvwindow_cmd_base: [TicCmd; NET_MAXPLAYERS],
}

impl Bot {
    pub fn new(settings: Option<GameSettings>) -> Self {
        Self {
            rng: thread_rng(),
            settings,
            recvwindow_cmd_base: [TicCmd::default(); NET_MAXPLAYERS],
        }
    }

    pub fn init(&mut self) {}

    pub fn build_ticcmd(&mut self, ticcmd: &mut TicCmd, _maketic: u32) {
        if self
            .settings
            .as_ref()
            .map_or(false, |s| s.consoleplayer >= 0)
        {
            ticcmd.forwardmove = self.rng.gen_range(-50..50);
            ticcmd.sidemove = self.rng.gen_range(-50..50);
            ticcmd.angleturn = self.rng.gen_range(-1024..1024);

            if self.rng.gen_bool(0.1) {
                ticcmd.buttons |= 1;
            }

            ticcmd.consistancy = self.settings.as_ref().map_or(0, |s| {
                self.recvwindow_cmd_base[s.consoleplayer as usize].consistancy
            });
        }
    }
}
