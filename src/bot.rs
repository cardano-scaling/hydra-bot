use rand::prelude::*;

use crate::net::{ClientState, GameSettings, TicCmd, NET_MAXPLAYERS};

pub struct Bot {
    rng: ThreadRng,
    settings: Option<GameSettings>,
}

impl Bot {
    pub fn new(settings: Option<GameSettings>) -> Self {
        Self {
            rng: thread_rng(),
            settings,
        }
    }

    pub fn init(&mut self) {}

    pub fn tick(
        &mut self,
        state: ClientState,
        last_ticcmd: TicCmd,
        window: [TicCmd; NET_MAXPLAYERS],
    ) -> TicCmd {
        if state == ClientState::InGame {
            let mut next = TicCmd::default();

            next.forwardmove = self.rng.gen_range(-50..50);
            next.sidemove = self.rng.gen_range(-50..50);
            next.angleturn = self.rng.gen_range(-1024..1024);

            // Randomly fire
            if self.rng.gen_bool(0.1) {
                next.buttons |= 1;
            }

            next.consistancy =
                window[self.settings.unwrap_or_default().consoleplayer as usize].consistancy;

            next
        } else {
            last_ticcmd
        }
    }
}
