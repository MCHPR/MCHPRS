use crate::chat::{ChatColor, ChatComponentBuilder, ColorCode};
use crate::network::packets::clientbound::{
    CDisplayScoreboard, CScoreboardObjective, CUpdateScore, ClientBoundPacket,
};
use crate::player::{PacketSender, Player};

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

#[derive(Default)]
pub struct Scoreboard {
    status_changed: Option<RedpilerState>,

    redpiler_state: RedpilerState,
}

impl Scoreboard {
    fn make_update_packet(&self) -> CUpdateScore {
        CUpdateScore {
            entity_name: self.redpiler_state.to_str().to_string(),
            action: 0,
            objective_name: "redpiler_status".to_string(),
            value: 1,
        }
    }

    fn make_removal_packet(&self, old_status: RedpilerState) -> CUpdateScore {
        CUpdateScore {
            entity_name: old_status.to_str().to_string(),
            action: 1,
            objective_name: "redpiler_status".to_string(),
            value: 0,
        }
    }

    pub fn display(&self, player: &Player) {
        player.send_packet(
            &CScoreboardObjective {
                objective_name: "redpiler_status".into(),
                mode: 0,
                objective_value: ChatComponentBuilder::new("Redpiler Status".into())
                    .color_code(ColorCode::Red)
                    .finish()
                    .encode_json(),
                ty: 0,
            }
            .encode(),
        );
        player.send_packet(
            &CDisplayScoreboard {
                position: 1,
                score_name: "redpiler_status".into(),
            }
            .encode(),
        );
        player.send_packet(&self.make_update_packet().encode());
    }

    pub fn update(&mut self, players: &[Player]) {
        let old_status = match self.status_changed {
            Some(old_status) => old_status,
            None => return,
        };
        self.status_changed = None;

        let removal_packet = self.make_removal_packet(old_status).encode();
        let update_packet = self.make_update_packet().encode();

        for player in players {
            player.send_packet(&removal_packet);
            player.send_packet(&update_packet);
        }
    }

    pub fn set_redpiler_state(&mut self, state: RedpilerState) {
        if state != self.redpiler_state {
            self.status_changed = Some(self.redpiler_state);
            self.redpiler_state = state;
        }
    }

    pub fn remove_player(&mut self, player: &Player) {
        let removal_packet = self.make_removal_packet(self.redpiler_state).encode();
        player.send_packet(&removal_packet);
    }
}
