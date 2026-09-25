//! Typed HTTP client for the LexHack backend.
//!
//! Every method returns `Result<T, ApiError>`. The client attaches the
//! bearer token on every authenticated request and translates the
//! backend's error envelope into a typed [`ApiError`].
//!
//! No method here ever sends a secret to the browser. The DTOs mirror
//! exactly what the backend returns; where the backend hides a field,
//! the DTO simply does not have it.

use gloo_net::http::{Request, RequestBuilder};
use serde::Serialize;
use uuid::Uuid;
use web_sys::wasm_bindgen::JsCast;

use crate::api::config::API_BASE_URL;
use crate::api::error::{ApiError, ApiErrorBody};
use crate::api::models::*;

/// The HTTP method we need, mapped to a gloo-net constructor.
#[derive(Debug, Clone, Copy)]
enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

/// A thin, cheaply-cloneable API client.
///
/// The client is stateless apart from the bearer token, which is stored
/// in memory only. Persistent storage of the token is a concern of
/// `crate::auth`, not of this module.
#[derive(Debug, Clone, Default)]
pub struct ApiClient {
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn new() -> Self {
        Self {
            base_url: API_BASE_URL.trim_end_matches('/').to_string(),
            token: None,
        }
    }

    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            base_url: API_BASE_URL.trim_end_matches('/').to_string(),
            token: Some(token.into()),
        }
    }

    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    // ---------------------------------------------------------------------
    // Request helpers
    // ---------------------------------------------------------------------

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn prepare(&self, method: Method, path: &str) -> RequestBuilder {
        let url = self.url(path);
        let builder = match method {
            Method::Get => Request::get(&url),
            Method::Post => Request::post(&url),
            Method::Patch => Request::patch(&url),
            Method::Delete => Request::delete(&url),
        };
        match &self.token {
            Some(token) => builder.header("Authorization", &format!("Bearer {token}")),
            None => builder,
        }
    }

    async fn send_json<B, T>(builder: RequestBuilder, body: Option<&B>) -> Result<T, ApiError>
    where
        B: Serialize + ?Sized,
        T: for<'de> serde::Deserialize<'de>,
    {
        let request = match body {
            Some(b) => builder
                .header("Content-Type", "application/json")
                .json(b)
                .map_err(|e| ApiError::Network(format!("serialize: {e}")))?,
            None => builder
                .build()
                .map_err(|e| ApiError::Network(format!("build: {e}")))?,
        };

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(format!("send: {e}")))?;

        let status = response.status();
        let retry_after = parse_retry_after(&response);

        if (200..300).contains(&status) {
            let parsed: T = response
                .json()
                .await
                .map_err(|e| ApiError::Network(format!("decode: {e}")))?;
            return Ok(parsed);
        }

        let parsed_body = read_error_body(response).await;
        Err(ApiError::from_status(status, parsed_body, retry_after))
    }

    /// Like `send_json` but for endpoints that return `204 No Content`.
    async fn send_no_content(builder: RequestBuilder) -> Result<(), ApiError> {
        let request = builder
            .build()
            .map_err(|e| ApiError::Network(format!("build: {e}")))?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(format!("send: {e}")))?;

        let status = response.status();
        if (200..300).contains(&status) {
            return Ok(());
        }

        let retry_after = parse_retry_after(&response);
        let parsed_body = read_error_body(response).await;
        Err(ApiError::from_status(status, parsed_body, retry_after))
    }

    // ---------------------------------------------------------------------
    // Auth
    // ---------------------------------------------------------------------

    pub async fn register(&self, req: &RegisterRequest) -> Result<AuthResponse, ApiError> {
        Self::send_json(self.prepare(Method::Post, "/api/auth/register"), Some(req)).await
    }

    pub async fn login(&self, req: &LoginRequest) -> Result<AuthResponse, ApiError> {
        Self::send_json(self.prepare(Method::Post, "/api/auth/login"), Some(req)).await
    }

    pub async fn logout(&self) -> Result<(), ApiError> {
        Self::send_no_content(self.prepare(Method::Post, "/api/auth/logout")).await
    }

    pub async fn me(&self) -> Result<UserResponse, ApiError> {
        Self::send_json::<(), _>(self.prepare(Method::Get, "/api/auth/me"), None).await
    }

    // ---------------------------------------------------------------------
    // Dashboard
    // ---------------------------------------------------------------------

    pub async fn dashboard_summary(&self) -> Result<DashboardSummary, ApiError> {
        Self::send_json::<(), _>(self.prepare(Method::Get, "/api/dashboard/summary"), None).await
    }

    // ---------------------------------------------------------------------
    // Contracts
    // ---------------------------------------------------------------------

    pub async fn list_contracts(
        &self,
        page: u32,
        limit: u32,
        sort: Option<&str>,
        order: Option<&str>,
    ) -> Result<PaginatedContracts, ApiError> {
        let mut qs = format!("?page={page}&limit={limit}");
        if let Some(s) = sort {
            qs.push_str(&format!("&sort={s}"));
        }
        if let Some(o) = order {
            qs.push_str(&format!("&order={o}"));
        }
        let path = format!("/api/contracts{qs}");
        Self::send_json::<(), _>(self.prepare(Method::Get, &path), None).await
    }

    pub async fn get_contract(&self, id: Uuid) -> Result<ContractResponse, ApiError> {
        let path = format!("/api/contracts/{id}");
        Self::send_json::<(), _>(self.prepare(Method::Get, &path), None).await
    }

    /// Uploads an image (JPEG, PNG, WebP) to
    /// `POST /api/contracts/extract-image` and returns the extracted
    /// text. Uses the same multipart pattern as `extract_text_from_pdf`.
    pub async fn extract_text_from_image(&self, file: web_sys::File) -> Result<String, ApiError> {
        let url = self.url("/api/contracts/extract-image");

        let form = web_sys::FormData::new()
            .map_err(|_| ApiError::Network("could not create FormData".into()))?;
        form.append_with_blob_and_filename("file", &file, &file.name())
            .map_err(|_| ApiError::Network("could not attach file".into()))?;

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(&form);
        if let Some(token) = self.token() {
            let headers =
                web_sys::Headers::new().map_err(|_| ApiError::Network("headers".into()))?;
            headers
                .set("Authorization", &format!("Bearer {token}"))
                .map_err(|_| ApiError::Network("auth header".into()))?;
            opts.set_headers(&headers);
        }

        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| ApiError::Network("could not build request".into()))?;

        let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
        let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| ApiError::Network("network error".into()))?;

        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| ApiError::Network("bad response".into()))?;

        let status = resp.status();
        if !(200..300).contains(&status) {
            let body = wasm_bindgen_futures::JsFuture::from(
                resp.json().map_err(|_| ApiError::Network("body".into()))?,
            )
            .await
            .ok()
            .and_then(|v| {
                serde_wasm_bindgen::from_value::<crate::api::error::ApiErrorBody>(v).ok()
            });
            return Err(ApiError::from_status(status as u16, body, None));
        }

        let json_value = wasm_bindgen_futures::JsFuture::from(
            resp.json().map_err(|_| ApiError::Network("json".into()))?,
        )
        .await
        .map_err(|_| ApiError::Network("json parse".into()))?;

        #[derive(serde::Deserialize)]
        struct ExtractResp {
            text: String,
        }

        let parsed: ExtractResp = serde_wasm_bindgen::from_value(json_value)
            .map_err(|_| ApiError::Network("unexpected response shape".into()))?;
        Ok(parsed.text)
    }

    /// Uploads a PDF to `POST /api/contracts/extract-text` and returns
    /// the extracted text. Does **not** create a contract — the caller
    /// feeds the returned text into the normal create flow.
    ///
    /// Uses `FormData` for the multipart body. The browser sets the
    /// correct `Content-Type` boundary automatically; do not override it.
    pub async fn extract_text_from_pdf(&self, file: web_sys::File) -> Result<String, ApiError> {
        let url = self.url("/api/contracts/extract-text");

        let form = web_sys::FormData::new()
            .map_err(|_| ApiError::Network("could not create FormData".into()))?;
        form.append_with_blob_and_filename("file", &file, &file.name())
            .map_err(|_| ApiError::Network("could not attach file".into()))?;

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(&form);
        if let Some(token) = self.token() {
            // Build headers with the auth token. We do not set
            // Content-Type — the browser must add the multipart boundary.
            let headers =
                web_sys::Headers::new().map_err(|_| ApiError::Network("headers".into()))?;
            headers
                .set("Authorization", &format!("Bearer {token}"))
                .map_err(|_| ApiError::Network("auth header".into()))?;
            opts.set_headers(&headers);
        }

        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| ApiError::Network("could not build request".into()))?;

        let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
        let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| ApiError::Network("network error".into()))?;

        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| ApiError::Network("bad response".into()))?;

        let status = resp.status();
        if !(200..300).contains(&status) {
            let body = wasm_bindgen_futures::JsFuture::from(
                resp.json().map_err(|_| ApiError::Network("body".into()))?,
            )
            .await
            .ok()
            .and_then(|v| {
                serde_wasm_bindgen::from_value::<crate::api::error::ApiErrorBody>(v).ok()
            });
            return Err(ApiError::from_status(status as u16, body, None));
        }

        let json_value = wasm_bindgen_futures::JsFuture::from(
            resp.json().map_err(|_| ApiError::Network("json".into()))?,
        )
        .await
        .map_err(|_| ApiError::Network("json parse".into()))?;

        #[derive(serde::Deserialize)]
        struct ExtractResp {
            text: String,
        }

        let parsed: ExtractResp = serde_wasm_bindgen::from_value(json_value)
            .map_err(|_| ApiError::Network("unexpected response shape".into()))?;
        Ok(parsed.text)
    }

    pub async fn create_contract(
        &self,
        req: &CreateContractRequest,
    ) -> Result<ContractResponse, ApiError> {
        Self::send_json(self.prepare(Method::Post, "/api/contracts"), Some(req)).await
    }

    pub async fn update_contract(
        &self,
        id: Uuid,
        req: &UpdateContractRequest,
    ) -> Result<ContractResponse, ApiError> {
        let path = format!("/api/contracts/{id}");
        Self::send_json(self.prepare(Method::Patch, &path), Some(req)).await
    }

    pub async fn delete_contract(&self, id: Uuid) -> Result<(), ApiError> {
        let path = format!("/api/contracts/{id}");
        Self::send_no_content(self.prepare(Method::Delete, &path)).await
    }

    pub async fn analyze_contract(&self, id: Uuid) -> Result<AnalysisResponse, ApiError> {
        let path = format!("/api/contracts/{id}/analyze");
        Self::send_json::<(), _>(self.prepare(Method::Post, &path), None).await
    }

    /// Fetches the *current* analysis without re-running the AI.
    pub async fn get_analysis(&self, id: Uuid) -> Result<AnalysisResponse, ApiError> {
        let path = format!("/api/contracts/{id}/analysis");
        Self::send_json::<(), _>(self.prepare(Method::Get, &path), None).await
    }

    // ---------------------------------------------------------------------
    // Reminders
    // ---------------------------------------------------------------------

    pub async fn generate_reminders(
        &self,
        contract_id: Uuid,
    ) -> Result<GenerationResponse, ApiError> {
        let path = format!("/api/contracts/{contract_id}/reminders/generate");
        Self::send_json::<(), _>(self.prepare(Method::Post, &path), None).await
    }

    pub async fn list_reminders(
        &self,
        contract_id: Uuid,
        page: u32,
        limit: u32,
    ) -> Result<PaginatedReminders, ApiError> {
        let path = format!("/api/contracts/{contract_id}/reminders?page={page}&limit={limit}");
        Self::send_json::<(), _>(self.prepare(Method::Get, &path), None).await
    }

    pub async fn create_custom_reminder(
        &self,
        contract_id: Uuid,
        req: &CreateCustomReminderRequest,
    ) -> Result<ReminderResponse, ApiError> {
        let path = format!("/api/contracts/{contract_id}/reminders");
        Self::send_json(self.prepare(Method::Post, &path), Some(req)).await
    }

    pub async fn cancel_reminder(&self, reminder_id: Uuid) -> Result<ReminderResponse, ApiError> {
        let path = format!("/api/reminders/{reminder_id}/cancel");
        Self::send_json::<(), _>(self.prepare(Method::Post, &path), None).await
    }

    // ---------------------------------------------------------------------
    // Notification channels
    // ---------------------------------------------------------------------

    pub async fn list_channels(
        &self,
        page: u32,
        limit: u32,
    ) -> Result<PaginatedChannels, ApiError> {
        let path = format!("/api/notification-channels?page={page}&limit={limit}");
        Self::send_json::<(), _>(self.prepare(Method::Get, &path), None).await
    }

    pub async fn get_channel(&self, id: Uuid) -> Result<ChannelResponse, ApiError> {
        let path = format!("/api/notification-channels/{id}");
        Self::send_json::<(), _>(self.prepare(Method::Get, &path), None).await
    }

    pub async fn create_channel(
        &self,
        body: &serde_json::Value,
    ) -> Result<ChannelResponse, ApiError> {
        Self::send_json(
            self.prepare(Method::Post, "/api/notification-channels"),
            Some(body),
        )
        .await
    }

    pub async fn update_channel(
        &self,
        id: Uuid,
        body: &serde_json::Value,
    ) -> Result<ChannelResponse, ApiError> {
        let path = format!("/api/notification-channels/{id}");
        Self::send_json(self.prepare(Method::Patch, &path), Some(body)).await
    }

    pub async fn delete_channel(&self, id: Uuid) -> Result<(), ApiError> {
        let path = format!("/api/notification-channels/{id}");
        Self::send_no_content(self.prepare(Method::Delete, &path)).await
    }

    pub async fn test_channel(&self, id: Uuid) -> Result<TestResultResponse, ApiError> {
        let path = format!("/api/notification-channels/{id}/test");
        Self::send_json::<(), _>(self.prepare(Method::Post, &path), None).await
    }
}

/// Reads the `Retry-After` header from a `gloo_net::http::Response` as
/// a number of seconds. gloo-net does not expose typed header
/// accessors, so we fall back to raw string parsing.
fn parse_retry_after(response: &gloo_net::http::Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.parse::<u64>().ok())
}

/// Consumes the response and tries to parse the backend's JSON error
/// envelope. Any failure (empty body, non-JSON, network error during
/// read) yields `None`, and the caller uses a status-based fallback.
async fn read_error_body(response: gloo_net::http::Response) -> Option<ApiErrorBody> {
    let text = response.text().await.ok()?;
    serde_json::from_str(&text).ok()
}
