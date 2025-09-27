use super::{MC_DATA_VERSION, MC_VERSION};
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Version {
    id: i32,
    name: String,
    series: String,
    snapshot: bool,
}

#[derive(Serialize)]
struct GeneratorLayer {
    block: String,
    height: i32,
}

#[derive(Serialize)]
struct GeneratorSettings {
    features: bool,
    lakes: bool,
    layers: Vec<GeneratorLayer>,
    biome: String,
}

#[derive(Serialize)]
struct DimensionGenerator {
    #[serde(rename = "type")]
    ty: String,
    settings: GeneratorSettings,
}

#[derive(Serialize)]
struct Dimension {
    #[serde(rename = "type")]
    ty: String,
    generator: DimensionGenerator,
}

#[derive(Serialize)]
struct WorldGenSettings {
    bonus_chest: bool,
    seed: i64,
    generate_features: bool,
    dimensions: HashMap<String, Dimension>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Player {
    #[serde(rename = "seenCredits")]
    seen_credits: bool,
    #[serde(rename = "death_time")]
    death_time: i16,
    #[serde(rename = "foodTickTimer")]
    food_tick_timer: i32,
    #[serde(rename = "xp_total")]
    xp_total: i32,
    #[serde(rename = "on_ground")]
    on_ground: bool,
    #[serde(rename = "absorption_amount")]
    absorption_amount: f32,
    #[serde(rename = "player_game_type")]
    player_game_type: i32,
    invulnerable: bool,
    selected_item_slot: i32,
    dimension: String,
    score: i32,
    // Rotation: [-14.148214340209961f, 4.684205532073975f],
    #[serde(rename = "hurt_by_timestamp")]
    hurt_by_timestamp: i32,
    #[serde(rename = "foodSaturationLevel")]
    food_saturation_level: f32,
    air: i16,
    // EnderItems: [],
    xp_seed: i32,
    #[serde(rename = "foodLevel")]
    food_level: i32,
    // UUID: [I;760560669,1319849827,-2121160272,-103375603],
    xp_level: i32,
    // Inventory: [],
    // Motion: [0.0d, -0.0784000015258789d, 0.0d],
    fall_distance: f32,
    data_version: i32,
    sleep_timer: i16,
    xp_p: f32,
    pos: Vec<f64>,
    // AABB: [-234.80000001192093d, 70.0d, -323.80000001192093d, -234.19999998807907d, 71.79999995231628d, -323.19999998807907d],
    health: f32,
    hurt_time: i16,
    fall_flying: bool,
    fire: i16,
    portal_cooldown: i32,
    #[serde(rename = "foodExhaustionLevel")]
    food_exhaustion_level: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LevelData {
    difficulty: i8,
    #[serde(rename = "thunderTime")]
    thunder_time: i32,
    border_size: f64,
    last_played: i64,
    #[serde(rename = "allowCommands")]
    allow_commands: bool,
    border_center_x: f64,
    #[serde(rename = "initialized")]
    initialized: bool,
    border_warning_blocks: f64,
    #[serde(rename = "hardcore")]
    hardcore: bool,
    #[serde(rename = "version")]
    version_num: i32,
    spawn_x: i32,
    game_type: i32,
    border_safe_zone: f64,
    spawn_angle: f32,
    level_name: String,
    time: i32,
    // ScheduledEvents: [],
    #[serde(rename = "clearWeatherTime")]
    clear_weather_time: i32,
    border_damage_per_block: f64,
    wandering_trader_spawn_delay: i32,
    #[serde(rename = "thundering")]
    thundering: bool,
    was_modded: bool,
    border_warning_time: f64,
    wandering_trader_spawn_chance: i32,
    spawn_y: i32,
    spawn_z: i32,
    border_size_lerp_time: i64,
    #[serde(rename = "raining")]
    raining: bool,
    world_gen_settings: WorldGenSettings,
    #[serde(rename = "rainTime")]
    rain_time: i32,
    data_version: i32,
    player: Player,
    difficulty_locked: bool,
    day_time: i64,
    border_center_z: f64,
    border_size_lerp_target: f64,
    version: Version,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LevelRoot {
    data: LevelData,
}

pub fn write_level_dat(level_name: &str, output_path: &Path) -> Result<()> {
    let mut dimensions = HashMap::new();
    dimensions.insert(
        "minecraft:overworld".to_string(),
        Dimension {
            ty: "minecraft:overworld".to_string(),
            generator: DimensionGenerator {
                ty: "minecraft:flat".to_string(),
                settings: GeneratorSettings {
                    features: false,
                    lakes: false,
                    layers: vec![
                        GeneratorLayer {
                            block: "minecraft:bedrock".to_string(),
                            height: 1,
                        },
                        GeneratorLayer {
                            block: "minecraft:stone".to_string(),
                            height: 3,
                        },
                        GeneratorLayer {
                            block: "minecraft:sandstone".to_string(),
                            height: 68,
                        },
                    ],
                    biome: "minecraft:desert".to_string(),
                },
            },
        },
    );

    let world_gen_settings = WorldGenSettings {
        bonus_chest: false,
        seed: 0,
        generate_features: false,
        dimensions,
    };

    let mut level_dat_file = File::create(output_path.join("level.dat")).unwrap();
    let level_dat = LevelData {
        difficulty: 0,
        thunder_time: 0,
        border_size: 59999968.0,
        last_played: 0,
        allow_commands: true,
        border_center_x: 0.0,
        initialized: true,
        border_warning_blocks: 5.0,
        hardcore: false,
        version_num: 19133,
        spawn_x: 0,
        game_type: 1,
        border_safe_zone: 5.0,
        spawn_angle: 0.0,
        level_name: level_name.to_string(),
        time: 0,
        clear_weather_time: 0,
        border_damage_per_block: 2.0,
        wandering_trader_spawn_delay: 24000,
        thundering: false,
        was_modded: false,
        border_warning_time: 15.0,
        wandering_trader_spawn_chance: 25,
        spawn_y: 100,
        spawn_z: 0,
        border_size_lerp_time: 0,
        raining: false,
        world_gen_settings,
        rain_time: i32::MAX,
        data_version: MC_DATA_VERSION,
        player: Player {
            seen_credits: false,
            death_time: 0,
            food_tick_timer: 0,
            on_ground: true,
            absorption_amount: 0.0,
            xp_total: 0,
            player_game_type: 1,
            invulnerable: false,
            selected_item_slot: 0,
            dimension: "minecraft:overworld".to_string(),
            score: 0,
            hurt_by_timestamp: 0,
            food_saturation_level: 5.0,
            air: 300,
            xp_seed: 0,
            food_level: 20,
            xp_level: 0,
            fall_distance: 0.0,
            data_version: MC_DATA_VERSION,
            sleep_timer: 0,
            xp_p: 0.0,
            pos: vec![100.0, 100.0, 100.0],
            health: 20.0,
            hurt_time: 0,
            fall_flying: false,
            fire: -20,
            portal_cooldown: 0,
            food_exhaustion_level: 0.0,
        },
        difficulty_locked: false,
        day_time: 6000,
        border_center_z: 0.0,
        border_size_lerp_target: 59999968.0,
        version: Version {
            id: MC_DATA_VERSION,
            name: MC_VERSION.to_string(),
            series: "main".to_string(),
            snapshot: false,
        },
    };
    nbt::to_gzip_writer(
        &mut level_dat_file,
        &LevelRoot { data: level_dat },
        Some("Data"),
    )
    .unwrap();
    Ok(())
}
