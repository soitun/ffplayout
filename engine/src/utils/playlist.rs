use std::path::PathBuf;

use log::*;
use tokio::fs;

use crate::file::norm_abs_path;
use crate::player::controller::ChannelManager;
use crate::player::utils::{JsonPlaylist, json_reader, json_writer};
use crate::utils::{config::PlayoutConfig, errors::ServiceError, generator::playlist_generator};

pub async fn read_playlist(
    config: &PlayoutConfig,
    date: String,
) -> Result<JsonPlaylist, ServiceError> {
    let d: Vec<&str> = date.split('-').collect();
    let mut playlist_path = config.channel.playlists.clone();

    playlist_path = playlist_path
        .join(d[0])
        .join(d[1])
        .join(date.clone())
        .with_extension("json");

    if !playlist_path.is_file() {
        return Err(ServiceError::NoContent());
    }

    match json_reader(&playlist_path).await {
        Ok(p) => Ok(p),
        Err(e) => Err(ServiceError::Conflict(e.to_string())),
    }
}

pub async fn write_playlist(
    config: &PlayoutConfig,
    json_data: JsonPlaylist,
) -> Result<String, ServiceError> {
    let date = json_data.date.clone();
    let d: Vec<&str> = date.split('-').collect();
    let mut playlist_path = config.channel.playlists.clone();

    if !playlist_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        playlist_path = playlist_path
            .join(d[0])
            .join(d[1])
            .join(date.clone())
            .with_extension("json");
    }

    let mut file_exists = false;

    if let Some(p) = playlist_path.parent() {
        fs::create_dir_all(p).await?;
    }

    if playlist_path.is_file() {
        file_exists = true;
        if let Ok(existing_data) = json_reader(&playlist_path).await {
            if json_data == existing_data {
                return Err(ServiceError::Conflict(format!(
                    "Playlist from {date}, already exists!"
                )));
            }
        }
    }

    match json_writer(&playlist_path, json_data).await {
        Ok(..) => {
            return if file_exists {
                Ok(format!("Update playlist from {date} success!"))
            } else {
                Ok(format!("Write playlist from {date} success!"))
            };
        }
        Err(e) => {
            error!("{e}");
        }
    }

    Err(ServiceError::InternalServerError)
}

pub async fn generate_playlist(manager: ChannelManager) -> Result<JsonPlaylist, ServiceError> {
    let (template, storage_root) = {
        let mut config = manager.config.write().await;
        (
            config.general.template.take(),
            config.channel.storage.clone(),
        )
    };

    if let Some(mut template) = template {
        for source in &mut template.sources {
            let mut resolved_paths = Vec::with_capacity(source.paths.len());

            for path in &source.paths {
                let (safe_path, _, _) = norm_abs_path(&storage_root, &path.to_string_lossy())?;
                resolved_paths.push(safe_path);
            }

            source.paths = resolved_paths;
        }

        {
            let mut config = manager.config.write().await;
            config.general.template = Some(template);
        }
    }

    match playlist_generator(&manager).await {
        Ok(playlists) => {
            if playlists.is_empty() {
                Err(ServiceError::Conflict(
                    "The playlist could not be written, maybe it already exists!".into(),
                ))
            } else {
                Ok(playlists[0].clone())
            }
        }
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn delete_playlist(config: &PlayoutConfig, date: &str) -> Result<String, ServiceError> {
    let d: Vec<&str> = date.split('-').collect();
    let mut playlist_path = PathBuf::from(&config.channel.playlists);

    playlist_path = playlist_path
        .join(d[0])
        .join(d[1])
        .join(date)
        .with_extension("json");

    if playlist_path.is_file() {
        match fs::remove_file(playlist_path).await {
            Ok(..) => Ok(format!("Delete playlist from {date} success!")),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        }
    } else {
        Ok(format!("No playlist to delete on: {date}"))
    }
}
