use crate::prowlarr::Download;
use anyhow::{Context as _, Result, bail};
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::multipart;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::OnceCell;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub save_path: String,
}

#[derive(Debug)]
pub struct QBittorrent {
    http: reqwest::Client,
    base: String,
    credentials: Option<(String, String)>,
    /// Logging in happens once; the SID cookie then lives in the client.
    session: OnceCell<()>,
}

impl QBittorrent {
    pub fn new(base: &str, user: Option<String>, pass: Option<String>) -> Result<Self> {
        let base = base.trim_end_matches('/').to_owned();
        let mut headers = HeaderMap::new();
        // CSRF check: writes are rejected if the Referer does not match.
        headers.insert(
            header::REFERER,
            HeaderValue::from_str(&base).context("invalid qBittorrent URL")?,
        );

        Ok(Self {
            http: reqwest::Client::builder()
                .default_headers(headers)
                .cookie_store(true)
                .timeout(TIMEOUT)
                .build()?,
            base,
            credentials: user.zip(pass),
            session: OnceCell::new(),
        })
    }

    /// No credentials means relying on qBittorrent's subnet auth bypass.
    async fn ensure_session(&self) -> Result<()> {
        self.session
            .get_or_try_init(|| async {
                let Some((user, pass)) = &self.credentials else {
                    return Ok(());
                };
                let body = self
                    .http
                    .post(format!("{}/api/v2/auth/login", self.base))
                    .form(&[("username", user), ("password", pass)])
                    .send()
                    .await
                    .context("qBittorrent is unreachable")?
                    .error_for_status()?
                    .text()
                    .await?;

                if !body.contains("Ok") {
                    bail!("qBittorrent rejected the credentials (QBIT_USER / QBIT_PASS)");
                }
                Ok(())
            })
            .await
            .copied()
    }

    pub async fn version(&self) -> Result<String> {
        self.ensure_session().await?;
        let resp = self
            .http
            .get(format!("{}/api/v2/app/version", self.base))
            .send()
            .await
            .context("qBittorrent is unreachable")?;
        reject_forbidden(&resp)?;
        Ok(resp.error_for_status()?.text().await?)
    }

    pub async fn categories(&self) -> Result<BTreeMap<String, Category>> {
        self.ensure_session().await?;
        let resp = self
            .http
            .get(format!("{}/api/v2/torrents/categories", self.base))
            .send()
            .await
            .context("qBittorrent is unreachable")?;
        reject_forbidden(&resp)?;
        resp.error_for_status()?
            .json()
            .await
            .context("unexpected categories payload")
    }

    /// Returns the save path used.
    ///
    /// The path must be sent explicitly: qBittorrent only honours a category's
    /// save path when Automatic Torrent Management is on, and it is off by
    /// default, so a category alone silently lands in the default folder.
    pub async fn add(
        &self,
        download: &Download,
        category: &str,
        save_path: Option<&str>,
        paused: bool,
    ) -> Result<String> {
        self.ensure_session().await?;

        let resolved = match save_path {
            Some(path) => path.to_owned(),
            None => self
                .categories()
                .await?
                .get(category)
                .map(|c| c.save_path.clone())
                .unwrap_or_default(),
        };

        let mut form = multipart::Form::new().text("category", category.to_owned());
        if !resolved.is_empty() {
            form = form.text("savepath", resolved.clone());
        }
        if paused {
            // `paused` is 4.x, `stopped` is 5.x.
            form = form.text("paused", "true").text("stopped", "true");
        }
        form = match download {
            Download::Magnet(url) => form.text("urls", url.clone()),
            Download::Torrent(bytes) => form.part(
                "torrents",
                multipart::Part::bytes(bytes.clone())
                    .file_name("release.torrent")
                    .mime_str("application/x-bittorrent")?,
            ),
        };

        let resp = self
            .http
            .post(format!("{}/api/v2/torrents/add", self.base))
            .multipart(form)
            .send()
            .await
            .context("qBittorrent is unreachable")?;
        reject_forbidden(&resp)?;

        let body = resp.error_for_status()?.text().await?;
        if !add_succeeded(&body) {
            bail!("qBittorrent refused the download: {}", body.trim());
        }
        Ok(resolved)
    }
}

fn reject_forbidden(resp: &reqwest::Response) -> Result<()> {
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        bail!("qBittorrent refused the request (403): set QBIT_USER / QBIT_PASS");
    }
    Ok(())
}

/// 4.x answers `Ok.`, 5.x a JSON result object.
fn add_succeeded(body: &str) -> bool {
    let body = body.trim();
    if body.is_empty() || body.contains("Ok") {
        return true;
    }

    #[derive(Deserialize)]
    struct AddResult {
        #[serde(default)]
        success_count: u32,
    }
    serde_json::from_str::<AddResult>(body).is_ok_and(|r| r.success_count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn anonymous(server: &MockServer) -> QBittorrent {
        QBittorrent::new(&server.uri(), None, None).unwrap()
    }

    fn authenticated(server: &MockServer) -> QBittorrent {
        QBittorrent::new(&server.uri(), Some("admin".into()), Some("secret".into())).unwrap()
    }

    async fn mock_categories(server: &MockServer, save_path: &str) {
        Mock::given(method("GET"))
            .and(path("/api/v2/torrents/categories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ebooks": { "name": "ebooks", "savePath": save_path }
            })))
            .mount(server)
            .await;
    }

    async fn mock_add(server: &MockServer, body: &str) {
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/add"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }

    async fn add_request_body(server: &MockServer) -> String {
        server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.url.path() == "/api/v2/torrents/add")
            .map(|r| String::from_utf8_lossy(&r.body).into_owned())
            .collect()
    }

    #[test]
    fn add_succeeded_accepts_both_api_generations() {
        assert!(add_succeeded("Ok."));
        assert!(add_succeeded(""));
        assert!(add_succeeded(
            r#"{"added_torrent_ids":["abc"],"success_count":1}"#
        ));
        assert!(!add_succeeded(r#"{"success_count":0,"failure_count":1}"#));
        assert!(!add_succeeded("Fails."));
    }

    #[test]
    fn a_url_with_illegal_bytes_is_refused_upfront() {
        assert!(QBittorrent::new("http://qbit\n.test", None, None).is_err());
    }

    #[test]
    fn the_base_url_keeps_no_trailing_slash() {
        assert_eq!(
            QBittorrent::new("http://qbit.test/", None, None)
                .unwrap()
                .base,
            "http://qbit.test"
        );
    }

    #[tokio::test]
    async fn version_is_returned_verbatim() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/app/version"))
            .respond_with(ResponseTemplate::new(200).set_body_string("v5.2.3"))
            .mount(&server)
            .await;

        assert_eq!(anonymous(&server).version().await.unwrap(), "v5.2.3");
    }

    #[tokio::test]
    async fn a_403_suggests_setting_credentials() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/app/version"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let error = anonymous(&server).version().await.unwrap_err().to_string();
        assert!(error.contains("QBIT_USER"), "got: {error}");
    }

    #[tokio::test]
    async fn an_unreachable_host_is_reported() {
        let error = QBittorrent::new("http://127.0.0.1:1", None, None)
            .unwrap()
            .version()
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("unreachable"), "got: {error}");
    }

    #[tokio::test]
    async fn categories_are_parsed_with_their_save_path() {
        let server = MockServer::start().await;
        mock_categories(&server, "/data/ebooks").await;

        let categories = anonymous(&server).categories().await.unwrap();
        assert_eq!(categories["ebooks"].save_path, "/data/ebooks");
        assert_eq!(categories["ebooks"].name, "ebooks");
    }

    #[tokio::test]
    async fn categories_reject_a_payload_they_cannot_read() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/torrents/categories"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        assert!(anonymous(&server).categories().await.is_err());
    }

    #[tokio::test]
    async fn categories_surface_a_403() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/torrents/categories"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        assert!(anonymous(&server).categories().await.is_err());
    }

    #[tokio::test]
    async fn add_resolves_the_category_save_path_and_sends_it() {
        let server = MockServer::start().await;
        mock_categories(&server, "/data/ebooks").await;
        mock_add(&server, "Ok.").await;

        let saved = anonymous(&server)
            .add(
                &Download::Magnet("magnet:?xt=urn:btih:abc".into()),
                "ebooks",
                None,
                false,
            )
            .await
            .unwrap();

        assert_eq!(saved, "/data/ebooks");
        let body = add_request_body(&server).await;
        assert!(
            body.contains("/data/ebooks"),
            "save path should be sent, got: {body}"
        );
        assert!(body.contains("ebooks"));
        assert!(body.contains("magnet:?xt=urn:btih:abc"));
    }

    #[tokio::test]
    async fn add_prefers_an_explicit_save_path_over_the_category_one() {
        let server = MockServer::start().await;
        mock_add(&server, "Ok.").await;

        let saved = anonymous(&server)
            .add(
                &Download::Magnet("magnet:?x".into()),
                "ebooks",
                Some("/elsewhere"),
                false,
            )
            .await
            .unwrap();

        assert_eq!(saved, "/elsewhere");
        let looked_up = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/api/v2/torrents/categories");
        assert!(!looked_up);
    }

    #[tokio::test]
    async fn add_omits_the_save_path_when_the_category_has_none() {
        let server = MockServer::start().await;
        mock_categories(&server, "").await;
        mock_add(&server, "Ok.").await;

        let saved = anonymous(&server)
            .add(&Download::Magnet("magnet:?x".into()), "ebooks", None, false)
            .await
            .unwrap();

        assert_eq!(saved, "");
        assert!(!add_request_body(&server).await.contains("savepath"));
    }

    #[tokio::test]
    async fn add_falls_back_to_no_path_for_an_unknown_category() {
        let server = MockServer::start().await;
        mock_categories(&server, "/data/ebooks").await;
        mock_add(&server, "Ok.").await;

        let saved = anonymous(&server)
            .add(
                &Download::Magnet("magnet:?x".into()),
                "missing",
                None,
                false,
            )
            .await
            .unwrap();
        assert_eq!(saved, "");
    }

    #[tokio::test]
    async fn add_sends_both_pause_spellings() {
        let server = MockServer::start().await;
        mock_add(&server, "Ok.").await;

        anonymous(&server)
            .add(
                &Download::Magnet("magnet:?x".into()),
                "ebooks",
                Some("/data"),
                true,
            )
            .await
            .unwrap();

        let body = add_request_body(&server).await;
        assert!(body.contains("paused"), "4.x spelling missing: {body}");
        assert!(body.contains("stopped"), "5.x spelling missing: {body}");
    }

    #[tokio::test]
    async fn add_uploads_a_torrent_file_as_multipart() {
        let server = MockServer::start().await;
        mock_add(&server, "Ok.").await;

        anonymous(&server)
            .add(
                &Download::Torrent(b"d8:announce".to_vec()),
                "ebooks",
                Some("/data"),
                false,
            )
            .await
            .unwrap();

        let body = add_request_body(&server).await;
        assert!(body.contains("release.torrent"), "got: {body}");
        assert!(body.contains("d8:announce"));
    }

    #[tokio::test]
    async fn add_accepts_the_json_answer_of_qbittorrent_5() {
        let server = MockServer::start().await;
        mock_add(
            &server,
            r#"{"added_torrent_ids":["abc"],"success_count":1,"failure_count":0}"#,
        )
        .await;

        assert!(
            anonymous(&server)
                .add(
                    &Download::Magnet("magnet:?x".into()),
                    "ebooks",
                    Some("/data"),
                    false
                )
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn add_reports_a_refusal() {
        let server = MockServer::start().await;
        mock_add(&server, r#"{"success_count":0,"failure_count":1}"#).await;

        let error = anonymous(&server)
            .add(
                &Download::Magnet("magnet:?x".into()),
                "ebooks",
                Some("/data"),
                false,
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("refused the download"), "got: {error}");
    }

    #[tokio::test]
    async fn add_surfaces_a_403() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/add"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let error = anonymous(&server)
            .add(
                &Download::Magnet("magnet:?x".into()),
                "ebooks",
                Some("/data"),
                false,
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("403"), "got: {error}");
    }

    #[tokio::test]
    async fn add_surfaces_a_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/add"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        assert!(
            anonymous(&server)
                .add(
                    &Download::Magnet("magnet:?x".into()),
                    "ebooks",
                    Some("/data"),
                    false
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn credentials_trigger_a_login_before_the_first_call() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .and(body_string_contains("username=admin"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
            .expect(1)
            .mount(&server)
            .await;
        mock_categories(&server, "/data/ebooks").await;

        let client = authenticated(&server);
        client.categories().await.unwrap();
        client.categories().await.unwrap();
    }

    #[tokio::test]
    async fn a_rejected_login_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
            .mount(&server)
            .await;

        let error = authenticated(&server)
            .version()
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("rejected the credentials"), "got: {error}");
    }

    #[tokio::test]
    async fn a_failing_login_endpoint_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        assert!(authenticated(&server).version().await.is_err());
    }

    #[tokio::test]
    async fn no_credentials_means_no_login_request() {
        let server = MockServer::start().await;
        mock_categories(&server, "/data/ebooks").await;

        anonymous(&server).categories().await.unwrap();
        let tried_login = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/api/v2/auth/login");
        assert!(!tried_login);
    }
}
