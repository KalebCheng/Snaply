use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const RELEASES_API: &str = "https://api.github.com/repos/KalebCheng/Snaply/releases/latest";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    latest_version: Option<String>,
    has_update: bool,
    release_url: Option<String>,
    download_url: Option<String>,
    published_at: Option<String>,
    notes: Option<String>,
}

fn normalized_version(tag: &str) -> Option<String> {
    let value = tag.trim().trim_start_matches(['v', 'V']);
    Version::parse(value)
        .ok()
        .map(|version| version.to_string())
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (Version::parse(latest), Version::parse(current)) {
        (Ok(latest), Ok(current)) => latest > current,
        _ => false,
    }
}

fn asset_matches_platform(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    match std::env::consts::OS {
        "macos" => name.ends_with(".dmg") || name.ends_with(".app.tar.gz"),
        "windows" => name.ends_with(".msi") || name.ends_with(".exe"),
        "linux" => {
            name.ends_with(".appimage")
                || name.ends_with(".deb")
                || name.ends_with(".rpm")
                || name.ends_with(".tar.gz")
        }
        _ => false,
    }
}

fn asset_matches_arch(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let arch_tokens: &[&str] = match std::env::consts::ARCH {
        "aarch64" => &["aarch64", "arm64", "universal"],
        "x86_64" => &["x86_64", "x64", "amd64", "universal"],
        _ => return false,
    };
    arch_tokens.iter().any(|token| name.contains(token))
}

fn preferred_download(release: &GithubRelease) -> Option<String> {
    let platform_assets: Vec<&GithubAsset> = release
        .assets
        .iter()
        .filter(|asset| asset_matches_platform(&asset.name))
        .collect();
    platform_assets
        .iter()
        .find(|asset| asset_matches_arch(&asset.name))
        .or_else(|| platform_assets.first())
        .map(|asset| asset.browser_download_url.clone())
        .or_else(|| Some(release.html_url.clone()))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateInfo, String> {
    let current_version = app.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(format!("Snaply/{current_version}"))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("GitHub release request failed: {error}"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateInfo {
            current_version,
            latest_version: None,
            has_update: false,
            release_url: None,
            download_url: None,
            published_at: None,
            notes: None,
        });
    }

    if !response.status().is_success() {
        return Err(format!(
            "GitHub release request returned {}",
            response.status()
        ));
    }

    let release = response
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("Invalid GitHub release response: {error}"))?;
    let latest_version = normalized_version(&release.tag_name);
    let has_update = latest_version
        .as_deref()
        .is_some_and(|latest| is_newer(latest, &current_version));

    Ok(UpdateInfo {
        current_version,
        latest_version,
        has_update,
        release_url: Some(release.html_url.clone()),
        download_url: preferred_download(&release),
        published_at: release.published_at,
        notes: release.body.filter(|body| !body.trim().is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semantic_versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert_eq!(normalized_version("v1.2.3").as_deref(), Some("1.2.3"));
    }
}
