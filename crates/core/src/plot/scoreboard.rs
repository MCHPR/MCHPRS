use crate::player::{PacketSender, Player};
use crate::redpiler::CompilerOptions;
use mchprs_network::packets::clientbound::{
    CDisplayObjective, CResetScore, CUpdateObjectives, CUpdateScore, ClientBoundPacket,
    ObjectiveNumberFormat,
};
use mchprs_text::{ColorCode, TextComponentBuilder};

#[derive(PartialEq, Eq, Default, Clone, Copy)]
pub enum RedpilerState {
    #[default]
    Stopped,
    Compiling,
    Running,
}

impl RedpilerState {
    fn to_str(self) -> &'static str {
        match self {
            RedpilerState::Stopped => "§d§lStopped",
            RedpilerState::Compiling => "§e§lCompiling",
            RedpilerState::Running => "§a§lRunning",
        }
    }
}

pub struct Scoreboard {
    current_state: Vec<String>,
}

impl Default for Scoreboard {
    fn default() -> Scoreboard {
        let mut sb = Scoreboard {
            current_state: vec![],
        };
        sb.set_redpiler_state(&[], RedpilerState::Stopped);
        sb
    }
}

impl Scoreboard {
    fn make_update_packet(&self, line: usize) -> CUpdateScore {
        CUpdateScore {
            entity_name: self.current_state[line].clone(),
            objective_name: "redpiler_status".to_string(),
            value: (self.current_state.len() - line) as i32,
            display_name: None,
            number_format: None,
        }
    }

    fn make_removal_packet(&self, line: usize) -> CResetScore {
        CResetScore {
            entity_name: self.current_state[line].clone(),
            objective_name: Some("redpiler_status".to_string()),
        }
    }

    fn set_lines(&mut self, players: &[Player], lines: Vec<String>) {
        for line in 0..self.current_state.len() {
            let removal_packet = self.make_removal_packet(line).encode();
            players.iter().for_each(|p| p.send_packet(&removal_packet));
        }

        self.current_state = lines;

        for line in 0..self.current_state.len() {
            let update_packet = self.make_update_packet(line).encode();
            players.iter().for_each(|p| p.send_packet(&update_packet));
        }
    }

    fn set_line(&mut self, players: &[Player], line: usize, text: String) {
        if line == self.current_state.len() {
            self.current_state.push(text);
        } else {
            let removal_packet = self.make_removal_packet(line).encode();
            players.iter().for_each(|p| p.send_packet(&removal_packet));

            self.current_state[line] = text;
        }

        let update_packet = self.make_update_packet(line).encode();
        players.iter().for_each(|p| p.send_packet(&update_packet));
    }

    pub fn add_player(&self, player: &Player) {
        player.send_packet(
            &CUpdateObjectives {
                objective_name: "redpiler_status".into(),
                mode: 0,
                objective_value: TextComponentBuilder::new("Redpiler Status".into())
                    .color_code(ColorCode::Red)
                    .finish(),
                ty: 0,
                number_format: Some(ObjectiveNumberFormat::Blank),
            }
            .encode(),
        );
        player.send_packet(
            &CDisplayObjective {
                position: 1,
                score_name: "redpiler_status".into(),
            }
            .encode(),
        );
        for i in 0..self.current_state.len() {
            player.send_packet(&self.make_update_packet(i).encode());
        }
    }

    pub fn remove_player(&mut self, player: &Player) {
        for i in 0..self.current_state.len() {
            player.send_packet(&self.make_removal_packet(i).encode());
        }
    }

    pub fn set_redpiler_state(&mut self, players: &[Player], state: RedpilerState) {
        self.set_line(players, 0, state.to_str().to_string());
    }

    pub fn set_redpiler_options(&mut self, players: &[Player], options: &CompilerOptions) {
        let mut new_lines = vec![self.current_state[0].clone()];

        let mut flags = Vec::new();
        if options.optimize {
            flags.push("§b- optimize");
        }
        if options.export {
            flags.push("§b- export");
        }
        if options.io_only {
            flags.push("§b- io only");
        }
        if options.update {
            flags.push("§b- update");
        }

        if !flags.is_empty() {
            new_lines.push("§7Flags:".to_string());
            new_lines.extend(flags.iter().map(|s| s.to_string()));
        }
        self.set_lines(players, new_lines);
    }
}
