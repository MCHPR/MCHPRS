use crate::utils::HyphenatedUUID;
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PlayerProfile {
    #[serde(rename = "id")]
    pub uuid: HyphenatedUUID,
    #[serde(rename = "name")]
    pub username: String,
}

impl PlayerProfile {
    pub async fn lookup_by_username(username: &str) -> Result<PlayerProfile> {
        let url = format!(
            "https://api.mojang.com/users/profiles/minecraft/{}",
            username
        );
        let client = reqwest::Client::new();
        let res = client
            .get(url)
            .send()
            .await?
            .json::<PlayerProfile>()
            .await?;
        Ok(res)
    }
}
