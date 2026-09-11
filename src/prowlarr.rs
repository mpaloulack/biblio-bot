use anyhow::{Context as _, Result, bail};
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::Deserialize;
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub title: String,
    pub guid: String,
    pub indexer_id: i64,
    #[serde(default)]
    pub indexer: String,
    #[serde(default)]
    pub size: u64,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub download_url: Option<String>,
    pub magnet_url: Option<String>,
    #[serde(default)]
    pub protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Download {
    Torrent(Vec<u8>),
    Magnet(String),
}

#[derive(Debug, Clone)]
pub struct Prowlarr {
    http: reqwest::Client,
    /// Does not follow redirects: reqwest cannot request a `magnet:` Location.
    downloader: reqwest::Client,
    base: String,
}

impl Prowlarr {
    pub fn new(base: &str, api_key: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let mut key = HeaderValue::from_str(api_key).context("invalid Prowlarr API key")?;
        key.set_sensitive(true);
        headers.insert("X-Api-Key", key);
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));

        let build = || {
            reqwest::Client::builder()
                .default_headers(headers.clone())
                .timeout(TIMEOUT)
        };

        Ok(Self {
            http: build().build()?,
            downloader: build()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base: base.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn ping(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Status {
            version: String,
        }

        let resp = self
            .http
            .get(format!("{}/api/v1/system/status", self.base))
            .send()
            .await
            .context("Prowlarr is unreachable")?;
        reject_unauthorized(&resp)?;

        let status: Status = resp
            .error_for_status()?
            .json()
            .await
            .context("unexpected status payload")?;
        Ok(status.version)
    }

    /// Best seeded first.
    pub async fn search(
        &self,
        query: &str,
        categories: &[u32],
        limit: u32,
    ) -> Result<Vec<Release>> {
        let mut params: Vec<(&str, String)> = vec![
            ("query", query.to_owned()),
            ("type", "search".to_owned()),
            ("limit", limit.to_string()),
        ];
        params.extend(categories.iter().map(|c| ("categories", c.to_string())));

        let resp = self
            .http
            .get(format!("{}/api/v1/search", self.base))
            .query(&params)
            .send()
            .await
            .context("the Prowlarr search failed")?;
        reject_unauthorized(&resp)?;

        let mut releases: Vec<Release> = resp
            .error_for_status()?
            .json()
            .await
            .context("unexpected search payload")?;
        releases.sort_by_key(|r| std::cmp::Reverse(r.seeders.unwrap_or(0)));
        Ok(releases)
    }

    /// Goes through Prowlarr so the indexer authentication is replayed for us.
    pub async fn fetch(&self, release: &Release) -> Result<Download> {
        let mut url = release
            .download_url
            .clone()
            .or_else(|| release.magnet_url.clone())
            .context("this release exposes neither a download link nor a magnet")?;

        for _ in 0..MAX_REDIRECTS {
            if url.starts_with("magnet:") {
                return Ok(Download::Magnet(url));
            }

            let resp = self
                .downloader
                .get(&url)
                .send()
                .await
                .context("the download link is unreachable")?;

            if resp.status().is_redirection() {
                let location = resp
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .context("redirect without a usable Location header")?;
                // Location may be relative; joining leaves an absolute magnet intact.
                url = reqwest::Url::parse(&url)
                    .context("unparsable download link")?
                    .join(location)
                    .context("unparsable redirect target")?
                    .to_string();
                continue;
            }

            let bytes = resp.error_for_status()?.bytes().await?;
            return Ok(Download::Torrent(bytes.to_vec()));
        }
        bail!("too many redirects on the download link")
    }
}

fn reject_unauthorized(resp: &reqwest::Response) -> Result<()> {
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("Prowlarr rejected the API key (401)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> Prowlarr {
        Prowlarr::new(&server.uri(), "test-key").unwrap()
    }

    fn release_json(title: &str, seeders: u32) -> serde_json::Value {
        json!({
            "title": title,
            "guid": format!("guid-{title}"),
            "indexerId": 1,
            "indexer": "TestIndexer",
            "size": 1024,
            "seeders": seeders,
            "leechers": 2,
            "downloadUrl": "http://example.test/file.torrent",
            "protocol": "torrent"
        })
    }

    fn release_with(download_url: Option<&str>, magnet: Option<&str>) -> Release {
        Release {
            title: "Test".to_owned(),
            guid: "guid".to_owned(),
            indexer_id: 1,
            indexer: "TestIndexer".to_owned(),
            size: 1024,
            seeders: Some(1),
            leechers: Some(0),
            download_url: download_url.map(str::to_owned),
            magnet_url: magnet.map(str::to_owned),
            protocol: "torrent".to_owned(),
        }
    }

    #[test]
    fn an_api_key_with_illegal_bytes_is_refused_upfront() {
        assert!(Prowlarr::new("http://prowlarr.test", "bad\nkey").is_err());
    }

    #[test]
    fn the_base_url_keeps_no_trailing_slash() {
        let client = Prowlarr::new("http://prowlarr.test/", "key").unwrap();
        assert_eq!(client.base, "http://prowlarr.test");
    }

    #[tokio::test]
    async fn ping_returns_the_reported_version() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/system/status"))
            .and(header("x-api-key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "version": "2.5.2" })))
            .mount(&server)
            .await;

        assert_eq!(client(&server).ping().await.unwrap(), "2.5.2");
    }

    #[tokio::test]
    async fn ping_calls_out_a_rejected_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/system/status"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let error = client(&server).ping().await.unwrap_err().to_string();
        assert!(error.contains("API key"), "got: {error}");
    }

    #[tokio::test]
    async fn ping_reports_a_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/system/status"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        assert!(client(&server).ping().await.is_err());
    }

    #[tokio::test]
    async fn ping_reports_an_unreachable_host() {
        let error = Prowlarr::new("http://127.0.0.1:1", "key")
            .unwrap()
            .ping()
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("unreachable"), "got: {error}");
    }

    #[tokio::test]
    async fn search_forwards_query_type_limit_and_categories() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search"))
            .and(query_param("query", "dune"))
            .and(query_param("type", "search"))
            .and(query_param("limit", "50"))
            .and(query_param("categories", "7020"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!([release_json("Dune", 5)])),
            )
            .mount(&server)
            .await;

        let found = client(&server).search("dune", &[7020], 50).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Dune");
    }

    #[tokio::test]
    async fn search_orders_results_by_seeders() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                release_json("few", 3),
                release_json("many", 42),
                release_json("none", 0),
            ])))
            .mount(&server)
            .await;

        let titles: Vec<String> = client(&server)
            .search("q", &[7020], 10)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.title)
            .collect();
        assert_eq!(titles, ["many", "few", "none"]);
    }

    #[tokio::test]
    async fn search_tolerates_releases_without_optional_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "title": "Usenet release",
                "guid": "guid",
                "indexerId": 7
            }])))
            .mount(&server)
            .await;

        let found = client(&server).search("q", &[7020], 10).await.unwrap();
        assert_eq!(found[0].seeders, None);
        assert_eq!(found[0].size, 0);
        assert_eq!(found[0].indexer, "");
    }

    #[tokio::test]
    async fn search_calls_out_a_rejected_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let error = client(&server)
            .search("q", &[7020], 10)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("API key"), "got: {error}");
    }

    #[tokio::test]
    async fn search_rejects_a_payload_it_cannot_read() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        assert!(client(&server).search("q", &[7020], 10).await.is_err());
    }

    #[tokio::test]
    async fn fetch_downloads_the_torrent_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/download"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"d8:announce".to_vec()))
            .mount(&server)
            .await;

        let release = release_with(Some(&format!("{}/download", server.uri())), None);
        let downloaded = client(&server).fetch(&release).await.unwrap();
        assert_eq!(downloaded, Download::Torrent(b"d8:announce".to_vec()));
    }

    #[tokio::test]
    async fn fetch_returns_a_magnet_without_any_request() {
        let server = MockServer::start().await;
        let release = release_with(None, Some("magnet:?xt=urn:btih:abc"));

        let downloaded = client(&server).fetch(&release).await.unwrap();
        assert_eq!(
            downloaded,
            Download::Magnet("magnet:?xt=urn:btih:abc".to_owned())
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_follows_a_redirect_to_a_magnet() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/download"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "magnet:?xt=urn:btih:redirected"),
            )
            .mount(&server)
            .await;

        let release = release_with(Some(&format!("{}/download", server.uri())), None);
        let downloaded = client(&server).fetch(&release).await.unwrap();
        assert_eq!(
            downloaded,
            Download::Magnet("magnet:?xt=urn:btih:redirected".to_owned())
        );
    }

    #[tokio::test]
    async fn fetch_follows_a_redirect_to_a_torrent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/download"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/real.torrent"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/real.torrent"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"torrent".to_vec()))
            .mount(&server)
            .await;

        let release = release_with(Some(&format!("{}/download", server.uri())), None);
        assert_eq!(
            client(&server).fetch(&release).await.unwrap(),
            Download::Torrent(b"torrent".to_vec())
        );
    }

    #[tokio::test]
    async fn fetch_gives_up_on_a_redirect_loop() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
            .mount(&server)
            .await;

        let release = release_with(Some(&format!("{}/loop", server.uri())), None);
        let error = client(&server)
            .fetch(&release)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("too many redirects"), "got: {error}");
    }

    #[tokio::test]
    async fn fetch_rejects_a_redirect_without_a_location() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/download"))
            .respond_with(ResponseTemplate::new(302))
            .mount(&server)
            .await;

        let release = release_with(Some(&format!("{}/download", server.uri())), None);
        let error = client(&server)
            .fetch(&release)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("Location"), "got: {error}");
    }

    #[tokio::test]
    async fn fetch_reports_a_failing_download_link() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/download"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let release = release_with(Some(&format!("{}/download", server.uri())), None);
        assert!(client(&server).fetch(&release).await.is_err());
    }

    #[tokio::test]
    async fn fetch_refuses_a_release_with_no_link_at_all() {
        let server = MockServer::start().await;
        let error = client(&server)
            .fetch(&release_with(None, None))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("neither"), "got: {error}");
    }
}
