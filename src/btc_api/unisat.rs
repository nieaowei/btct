use reqwest::header::HeaderMap;

pub struct Client {
    http: reqwest::Client,
    token: String,
}

impl Client {
    fn url() -> &'static str {
        ""
    }
    pub fn new(token: &str) -> Self {
        let headers = HeaderMap::new();

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        Self {
            http,
            token: token.to_string(),
        }
    }
}
