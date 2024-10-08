use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub data: Vec<u8>,
    pub pos: usize,
}

impl Packet {
    pub fn new() -> Self {
        Packet {
            data: Vec::new(),
            pos: 0,
        }
    }

    pub fn write_blob(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
    }

    pub fn write_u16(&mut self, value: u16) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    fn read_ticcmd_diff(&mut self, lowres_turn: bool) -> Option<TicDiff> {
        let mut diff = TicDiff {
            diff: self.read_u8()? as u32,
            ..Default::default()
        };

        if diff.diff & NET_TICDIFF_FORWARD != 0 {
            diff.cmd.forwardmove = self.read_i8()?;
        }

        if diff.diff & NET_TICDIFF_SIDE != 0 {
            diff.cmd.sidemove = self.read_i8()?;
        }

        if diff.diff & NET_TICDIFF_TURN != 0 {
            if lowres_turn {
                diff.cmd.angleturn = (self.read_i8()? as i16) * 256;
            } else {
                diff.cmd.angleturn = self.read_i16()?;
            }
        }

        if diff.diff & NET_TICDIFF_BUTTONS != 0 {
            diff.cmd.buttons = self.read_u8()?;
        }

        if diff.diff & NET_TICDIFF_CONSISTANCY != 0 {
            diff.cmd.consistancy = self.read_u8()?;
        }

        if diff.diff & NET_TICDIFF_CHATCHAR != 0 {
            diff.cmd.chatchar = self.read_u8()?;
        } else {
            diff.cmd.chatchar = 0;
        }

        if diff.diff & NET_TICDIFF_RAVEN != 0 {
            diff.cmd.lookfly = self.read_u8()?;
            diff.cmd.arti = self.read_u8()?;
        } else {
            diff.cmd.arti = 0;
        }

        if diff.diff & NET_TICDIFF_STRIFE != 0 {
            diff.cmd.buttons2 = self.read_u8()?;
            diff.cmd.inventory = self.read_i16()? as i32;
        } else {
            diff.cmd.inventory = 0;
        }

        Some(diff)
    }

    pub fn write_u8(&mut self, value: u8) {
        self.data.push(value);
    }

    pub fn write_i8(&mut self, value: i8) {
        self.write_u8(value as u8);
    }

    pub fn write_i16(&mut self, value: i16) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_i32(&mut self, value: i32) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_string(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0); // Null terminator
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        if self.pos < self.data.len() {
            let value = self.data[self.pos];
            self.pos += 1;
            Some(value)
        } else {
            None
        }
    }

    pub fn read_i8(&mut self) -> Option<i8> {
        self.read_u8().map(|v| v as i8)
    }

    pub fn read_u16(&mut self) -> Option<u16> {
        if self.pos + 2 <= self.data.len() {
            let bytes = &self.data[self.pos..self.pos + 2];
            self.pos += 2;
            Some(u16::from_be_bytes(bytes.try_into().unwrap()))
        } else {
            None
        }
    }

    pub fn read_i16(&mut self) -> Option<i16> {
        self.read_u16().map(|v| v as i16)
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        if self.pos + 4 <= self.data.len() {
            let bytes = &self.data[self.pos..self.pos + 4];
            self.pos += 4;
            Some(u32::from_be_bytes(bytes.try_into().unwrap()))
        } else {
            None
        }
    }

    pub fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|v| v as i32)
    }

    pub fn read_string(&mut self) -> Option<String> {
        if let Some(terminator) = self.data[self.pos..].iter().position(|&c| c == 0) {
            let bytes = &self.data[self.pos..self.pos + terminator];
            let string = String::from_utf8_lossy(bytes).into_owned();
            self.pos += terminator + 1; // Skip the NUL terminator
            Some(string)
        } else {
            None
        }
    }

    pub fn read_safe_string(&mut self) -> Option<String> {
        self.read_string().map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_graphic() || c.is_whitespace())
                .collect()
        })
    }

    fn read_sha1sum(&mut self, digest: &mut [u8; 20]) -> Option<()> {
        if self.pos + 20 <= self.data.len() {
            digest.copy_from_slice(&self.data[self.pos..self.pos + 20]);
            self.pos += 20;
            Some(())
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.pos = 0;
    }

    pub fn read_protocol(&mut self) -> Protocol {
        if let Some(name) = self.read_string() {
            match name.as_str() {
                "CHOCOLATE_DOOM_0" => Protocol::ChocolateDoom0,
                _ => Protocol::Unknown,
            }
        } else {
            Protocol::Unknown
        }
    }

    pub fn write_protocol_list(&mut self) {
        self.write_u8(1); // Number of protocols
        self.write_string("CHOCOLATE_DOOM_0");
    }

    pub fn write_protocol(&mut self, protocol: Protocol) {
        let name = match protocol {
            Protocol::ChocolateDoom0 => "CHOCOLATE_DOOM_0",
            _ => panic!("NET_WriteProtocol: Unknown protocol {:?}", protocol),
        };
        self.write_string(name);
    }

    pub fn write_connect_data(&mut self, data: &ConnectData) {
        self.write_u8(data.gamemode as u8);
        self.write_u8(data.gamemission as u8);
        self.write_u8(data.lowres_turn as u8);
        self.write_u8(data.drone as u8);
        self.write_u8(data.max_players as u8);
        self.write_u8(data.is_freedoom as u8);
        self.write_blob(&data.wad_sha1sum);
        self.write_blob(&data.deh_sha1sum);
        self.write_u8(data.player_class as u8);
    }

    pub fn read_wait_data(&mut self) -> Option<WaitData> {
        let mut data = WaitData {
            num_players: self.read_u8()? as i32,
            num_drones: self.read_u8()? as i32,
            ready_players: self.read_u8()? as i32,
            max_players: self.read_u8()? as i32,
            is_controller: self.read_u8()? as i32,
            consoleplayer: self.read_i8()? as i32,
            ..Default::default()
        };
        for i in 0..data.num_players as usize {
            let name = self.read_string()?;
            if name.len() >= MAXPLAYERNAME {
                return None;
            }
            data.player_names[i] = ['\0'; MAXPLAYERNAME];
            for (j, c) in name.chars().enumerate().take(MAXPLAYERNAME) {
                data.player_names[i][j] = c;
            }
            let addr = self.read_string()?;
            if addr.len() >= MAXPLAYERNAME {
                return None;
            }
            data.player_addrs[i] = ['\0'; MAXPLAYERNAME];
            for (j, c) in addr.chars().enumerate().take(MAXPLAYERNAME) {
                data.player_addrs[i][j] = c;
            }
        }
        self.read_sha1sum(&mut data.wad_sha1sum)?;
        self.read_sha1sum(&mut data.deh_sha1sum)?;
        data.is_freedoom = self.read_u8()? as i32;
        Some(data)
    }

    pub fn read_settings(&mut self) -> Option<GameSettings> {
        let mut settings = GameSettings {
            ticdup: self.read_u8()? as i32,
            extratics: self.read_u8()? as i32,
            deathmatch: self.read_u8()? as i32,
            nomonsters: self.read_u8()? as i32,
            fast_monsters: self.read_u8()? as i32,
            respawn_monsters: self.read_u8()? as i32,
            episode: self.read_u8()? as i32,
            map: self.read_u8()? as i32,
            skill: self.read_i8()? as i32,
            gameversion: self.read_u8()? as i32,
            lowres_turn: self.read_u8()? as i32,
            new_sync: self.read_u8()? as i32,
            timelimit: self.read_u32()?,
            loadgame: self.read_i8()? as i32,
            random: self.read_u8()? as i32,
            num_players: self.read_u8()? as i32,
            consoleplayer: self.read_i8()? as i32,
            ..Default::default()
        };
        for i in 0..settings.num_players as usize {
            settings.player_classes[i] = self.read_u8()? as i32;
        }
        Some(settings)
    }

    pub fn write_settings(&mut self, settings: &GameSettings) {
        self.write_u8(settings.ticdup as u8);
        self.write_u8(settings.extratics as u8);
        self.write_u8(settings.deathmatch as u8);
        self.write_u8(settings.nomonsters as u8);
        self.write_u8(settings.fast_monsters as u8);
        self.write_u8(settings.respawn_monsters as u8);
        self.write_u8(settings.episode as u8);
        self.write_u8(settings.map as u8);
        self.write_i8(settings.skill as i8);
        self.write_u8(settings.gameversion as u8);
        self.write_u8(settings.lowres_turn as u8);
        self.write_u8(settings.new_sync as u8);
        self.write_u32(settings.timelimit);
        self.write_i8(settings.loadgame as i8);
        self.write_u8(settings.random as u8);
        self.write_u8(settings.num_players as u8);
        self.write_i8(settings.consoleplayer as i8);
        for i in 0..settings.num_players as usize {
            self.write_u8(settings.player_classes[i] as u8);
        }
    }

    pub fn read_full_ticcmd(&mut self, lowres_turn: bool) -> Option<FullTicCmd> {
        let mut cmd = FullTicCmd {
            latency: self.read_i16()? as i32,
            ..Default::default()
        };

        let bitfield = self.read_u8()?;
        for i in 0..NET_MAXPLAYERS {
            cmd.playeringame[i] = (bitfield & (1 << i)) != 0;
        }

        for i in 0..NET_MAXPLAYERS {
            if cmd.playeringame[i] {
                cmd.cmds[i] = self.read_ticcmd_diff(lowres_turn)?;
            }
        }
        Some(cmd)
    }

    pub fn write_ticcmd_diff(&mut self, diff: &TicDiff, lowres_turn: bool) {
        self.write_u8(diff.diff as u8);

        if diff.diff & NET_TICDIFF_FORWARD != 0 {
            self.write_i8(diff.cmd.forwardmove);
        }

        if diff.diff & NET_TICDIFF_SIDE != 0 {
            self.write_i8(diff.cmd.sidemove);
        }

        if diff.diff & NET_TICDIFF_TURN != 0 {
            if lowres_turn {
                self.write_i8((diff.cmd.angleturn / 256) as i8);
            } else {
                self.write_i16(diff.cmd.angleturn);
            }
        }

        if diff.diff & NET_TICDIFF_BUTTONS != 0 {
            self.write_u8(diff.cmd.buttons);
        }

        if diff.diff & NET_TICDIFF_CONSISTANCY != 0 {
            self.write_u8(diff.cmd.consistancy);
        }

        if diff.diff & NET_TICDIFF_CHATCHAR != 0 {
            self.write_u8(diff.cmd.chatchar);
        }

        if diff.diff & NET_TICDIFF_RAVEN != 0 {
            self.write_u8(diff.cmd.lookfly);
            self.write_u8(diff.cmd.arti);
        }

        if diff.diff & NET_TICDIFF_STRIFE != 0 {
            self.write_u8(diff.cmd.buttons2);
            self.write_i16(diff.cmd.inventory as i16);
        }
    }
}
