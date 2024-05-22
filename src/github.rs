use reqwest::Error;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Release {
    pub tag_name: String,
    pub body: String,
    pub name: Option<String>,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize, Debug)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub id: i32,
}

pub async fn fetch_releases(owner: &str, repo: &str, token: &str) -> Result<Vec<Release>, Error> {
    let url = format!("https://api.github.com/repos/{}/{}/releases", owner, repo);
    let client = reqwest::Client::new();

    let auth_header = format!("Bearer {}", token);
    let response = client
        .get(&url)
        .header("User-Agent", "request")
        .header("Authorization", auth_header)
        .send()
        .await?
        .json::<Vec<Release>>()
        .await?;

    Ok(response)
}

pub async fn download_asset(
    owner: &str,
    repo: &str,
    token: &str,
    asset_id: i32,
    file_path: &str,
) -> Result<usize, Error> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/assets/{}",
        owner, repo, asset_id
    );

    let client = reqwest::Client::new();
    let auth_header = format!("Bearer {}", token);

    let response = client
        .get(&url)
        .header("User-Agent", "request")
        .header("Authorization", auth_header)
        .header("Accept", "application/octet-stream")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await?;

    let content = response.bytes().await?;

    let mut file = tokio::fs::File::create(file_path)
        .await
        .expect("Failed to create download file!");

    tokio::io::copy(&mut content.as_ref(), &mut file)
        .await
        .expect("Failed to copy the downloaded artifact to a local file!");

    Ok(content.len())
}
