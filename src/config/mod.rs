use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
mod types;

pub use types::*;

pub fn load_user_config() -> Result<Config> {
    let config_dir = get_config_directory()?;
    let config_file_path = config_dir.join("config.toml");

    // 确保配置目录存在
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("Failed to create config directory: {:?}", config_dir))?;

    if !config_file_path.exists() {
        create_default_config(&config_file_path)?;
    }

    // 读取并解析配置文件
    let config_content = fs::read_to_string(&config_file_path)
        .with_context(|| format!("Failed to read config file: {:?}", config_file_path))?;

    let config: Result<Config, toml::de::Error> = toml::from_str(&config_content);
    match config {
        Ok(cfg) => Ok(cfg),
        Err(e) => {
            // 解析失败，自动备份原配置并重建
            let bak_path = config_file_path.with_extension("bak");
            fs::rename(&config_file_path, &bak_path)
                .with_context(|| format!("Failed to backup old config to {:?}", bak_path))?;
            create_default_config(&config_file_path)?;
            let config_content = fs::read_to_string(&config_file_path).with_context(|| {
                format!("Failed to read new config file: {:?}", config_file_path)
            })?;
            let config: Config = toml::from_str(&config_content)
                .with_context(|| "Failed to parse new config file")?;
            println!(
                "Config parse error: {}. Old config has been backed up to {:?}, new config created.",
                e, bak_path
            );
            Ok(config)
        }
    }
}

fn get_config_directory() -> Result<PathBuf> {
    if let Some(config_dir) = dirs::config_dir() {
        Ok(config_dir.join("telembed"))
    } else {
        anyhow::bail!("Could not determine config directory")
    }
}

fn create_default_config(config_path: &PathBuf) -> Result<()> {
    let default_cfg = Config::default();
    // 序列化为 TOML
    let default_content = toml::to_string_pretty(&default_cfg)
        .map_err(|e| anyhow::anyhow!("Failed to serialize default config: {}", e))?;
    fs::write(config_path, default_content)
        .with_context(|| format!("Failed to write default config to {:?}", config_path))?;
    println!("Created default config file at: {:?}", config_path);
    Ok(())
}
