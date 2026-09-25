//! `/api/contracts/*` router and handlers.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::reminders;
use crate::auth::AuthenticatedUser;
use crate::db;
use crate::errors::{AppError, AppResult};
use crate::models::{Contract, ContractSummaryRow};
use crate::services::analysis;
use crate::services::contracts::{
    self as svc, SortDirection, SortField, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT,
};
use crate::state::AppState;

const CONTRACT_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;
/// Multipart bodies for PDF uploads are larger than JSON bodies.
const EXTRACT_BODY_LIMIT_BYTES: usize = 20 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    // The extract-text route accepts multipart bodies (PDF uploads)
    // which are larger than the JSON bodies used elsewhere. It gets its
    // own body limit.
    let upload = Router::new()
        .route("/extract-text", post(extract_text))
        .route("/extract-image", post(extract_image))
        .layer(DefaultBodyLimit::max(EXTRACT_BODY_LIMIT_BYTES));

    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).patch(update).delete(delete_one))
        .route("/:id/analyze", post(analyze))
        .route("/:id/analysis", get(get_analysis))
        .nest("/:id/reminders", reminders::contract_scoped_router())
        .merge(upload)
        .layer(DefaultBodyLimit::max(CONTRACT_BODY_LIMIT_BYTES))
}

// -------------------------------------------------------------------------
// DTOs (contracts)
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateContractRequest {
    pub title: String,
    pub raw_text: String,
}

/// Response from `POST /api/contracts/extract-text`.
#[derive(Debug, Serialize)]
pub struct ExtractTextResponse {
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateContractRequest {
    pub title: Option<String>,
    pub raw_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQueryRaw {
    pub page: Option<String>,
    pub limit: Option<String>,
    pub sort: Option<String>,
    pub order: Option<String>,
}

struct ParsedListQuery {
    page: u32,
    limit: u32,
    sort: SortField,
    direction: SortDirection,
}

#[derive(Debug, Serialize)]
pub struct ContractResponse {
    pub id: Uuid,
    pub title: String,
    pub raw_text: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risk_level: Option<String>,
    pub risk_score: Option<i32>,
    pub risk_summary: Option<String>,
    pub content_version: i32,
    pub analysis_status: String,
    pub analyzed_at: Option<DateTime<Utc>>,
    pub analyzed_content_version: Option<i32>,
    pub analysis_provider: Option<String>,
    pub analysis_model: Option<String>,
    pub analysis_prompt_version: Option<i32>,
    pub analysis_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Contract> for ContractResponse {
    fn from(c: Contract) -> Self {
        Self {
            id: c.id,
            title: c.title,
            raw_text: c.raw_text,
            start_date: c.start_date,
            end_date: c.end_date,
            risk_level: c.risk_level,
            risk_score: c.risk_score,
            risk_summary: c.risk_summary,
            content_version: c.content_version,
            analysis_status: c.analysis_status,
            analyzed_at: c.analyzed_at,
            analyzed_content_version: c.analyzed_content_version,
            analysis_provider: c.analysis_provider,
            analysis_model: c.analysis_model,
            analysis_prompt_version: c.analysis_prompt_version,
            analysis_error: c.analysis_error,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContractSummary {
    pub id: Uuid,
    pub title: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risk_level: Option<String>,
    pub risk_score: Option<i32>,
    pub risk_summary: Option<String>,
    pub content_version: i32,
    pub analysis_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ContractSummaryRow> for ContractSummary {
    fn from(c: ContractSummaryRow) -> Self {
        Self {
            id: c.id,
            title: c.title,
            start_date: c.start_date,
            end_date: c.end_date,
            risk_level: c.risk_level,
            risk_score: c.risk_score,
            risk_summary: c.risk_summary,
            content_version: c.content_version,
            analysis_status: c.analysis_status,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedContracts {
    pub items: Vec<ContractSummary>,
    pub page: u32,
    pub limit: u32,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct RiskItem {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub risk_level: String,
    pub risk_score: i32,
    pub evidence: String,
}

#[derive(Debug, Serialize)]
pub struct ObligationItem {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub due_date: Option<NaiveDate>,
    pub responsible_party: Option<String>,
    pub status: String,
    pub risk_level: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AnalysisResponse {
    pub contract_id: Uuid,
    pub analysis_status: String,
    pub content_version: i32,
    pub analyzed_content_version: Option<i32>,
    pub analyzed_at: Option<DateTime<Utc>>,
    pub analysis_provider: Option<String>,
    pub analysis_model: Option<String>,
    pub analysis_prompt_version: Option<i32>,
    pub analysis_error: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub risk_level: Option<String>,
    pub risk_score: Option<i32>,
    pub risk_summary: Option<String>,
    pub risks: Vec<RiskItem>,
    pub obligations: Vec<ObligationItem>,
}

// -------------------------------------------------------------------------
// Extractor rejection mapping
// -------------------------------------------------------------------------

fn map_json_rejection(err: JsonRejection) -> AppError {
    match err.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        StatusCode::UNPROCESSABLE_ENTITY => AppError::UnprocessableEntity(err.body_text()),
        _ => AppError::Validation(err.body_text()),
    }
}

fn map_path_rejection(_err: PathRejection) -> AppError {
    AppError::Validation("path parameter must be a valid UUID".to_string())
}

fn map_query_rejection(_err: QueryRejection) -> AppError {
    AppError::Validation("invalid query string".to_string())
}

// -------------------------------------------------------------------------
// Query parsing
// -------------------------------------------------------------------------

fn parse_list_query(raw: ListQueryRaw) -> Result<ParsedListQuery, AppError> {
    let page = match raw.page.as_deref() {
        Some(s) => s
            .parse::<u32>()
            .map_err(|_| AppError::Validation("`page` must be a positive integer".to_string()))?,
        None => 1,
    };
    if page == 0 {
        return Err(AppError::Validation("`page` must be >= 1".to_string()));
    }

    let limit = match raw.limit.as_deref() {
        Some(s) => s
            .parse::<u32>()
            .map_err(|_| AppError::Validation("`limit` must be a positive integer".to_string()))?,
        None => DEFAULT_PAGE_LIMIT,
    };
    if limit == 0 {
        return Err(AppError::Validation("`limit` must be >= 1".to_string()));
    }
    if limit > MAX_PAGE_LIMIT {
        return Err(AppError::Validation(format!(
            "`limit` must be <= {MAX_PAGE_LIMIT}"
        )));
    }

    let sort = match raw.sort.as_deref() {
        None | Some("created_at") => SortField::CreatedAt,
        Some("updated_at") => SortField::UpdatedAt,
        Some("end_date") => SortField::EndDate,
        Some("title") => SortField::Title,
        Some(other) => {
            return Err(AppError::Validation(format!(
                "`sort` must be one of: created_at, updated_at, end_date, title (got `{other}`)"
            )))
        }
    };

    let direction = match raw.order.as_deref() {
        None | Some("desc") => SortDirection::Desc,
        Some("asc") => SortDirection::Asc,
        Some(other) => {
            return Err(AppError::Validation(format!(
                "`order` must be `asc` or `desc` (got `{other}`)"
            )))
        }
    };

    Ok(ParsedListQuery {
        page,
        limit,
        sort,
        direction,
    })
}

// -------------------------------------------------------------------------
// Handlers
// -------------------------------------------------------------------------

pub async fn create(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    payload: Result<Json<CreateContractRequest>, JsonRejection>,
) -> AppResult<impl IntoResponse> {
    let Json(req) = payload.map_err(map_json_rejection)?;
    let title = svc::validate_title(&req.title)?;
    svc::validate_raw_text(&req.raw_text)?;
    let contract = svc::create_contract(&state, auth.user_id, title, req.raw_text).await?;
    Ok((StatusCode::CREATED, Json(ContractResponse::from(contract))))
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    query: Result<Query<ListQueryRaw>, QueryRejection>,
) -> AppResult<impl IntoResponse> {
    let Query(raw) = query.map_err(map_query_rejection)?;
    let parsed = parse_list_query(raw)?;
    let (rows, total) = svc::list_contracts(
        &state,
        auth.user_id,
        parsed.page,
        parsed.limit,
        parsed.sort,
        parsed.direction,
    )
    .await?;
    let items: Vec<ContractSummary> = rows.into_iter().map(ContractSummary::from).collect();
    Ok((
        StatusCode::OK,
        Json(PaginatedContracts {
            items,
            page: parsed.page,
            limit: parsed.limit,
            total,
        }),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(id) = path.map_err(map_path_rejection)?;
    let contract = svc::get_contract(&state, auth.user_id, id).await?;
    Ok((StatusCode::OK, Json(ContractResponse::from(contract))))
}

pub async fn update(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
    payload: Result<Json<UpdateContractRequest>, JsonRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(id) = path.map_err(map_path_rejection)?;
    let Json(req) = payload.map_err(map_json_rejection)?;
    let title = match req.title.as_deref() {
        Some(t) => Some(svc::validate_title(t)?),
        None => None,
    };
    let raw_text = match req.raw_text.as_deref() {
        Some(t) => {
            svc::validate_raw_text(t)?;
            Some(t.to_string())
        }
        None => None,
    };
    let updated = svc::update_contract(&state, auth.user_id, id, title, raw_text).await?;
    Ok((StatusCode::OK, Json(ContractResponse::from(updated))))
}

pub async fn delete_one(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(id) = path.map_err(map_path_rejection)?;
    svc::delete_contract(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/contracts/:id/analysis`
///
/// Returns the *current* analysis for a contract without re-running
/// the AI. If the contract has never been analyzed, the response still
/// has `analysis_status = "not_analyzed"` and empty risks/obligations.
pub async fn get_analysis(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    let Path(id) = path.map_err(map_path_rejection)?;
    let contract = svc::get_contract(&state, auth.user_id, id).await?;
    build_analysis_response(&state, contract).await
}

pub async fn analyze(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    path: Result<Path<Uuid>, PathRejection>,
) -> AppResult<impl IntoResponse> {
    // Per-user rate limit: AI analysis creates external cost.
    state
        .analyze_rate_limiter()
        .check(auth.user_id)
        .map_err(|retry| AppError::RateLimited {
            retry_after_seconds: retry.as_secs().max(1),
        })?;

    let Path(id) = path.map_err(map_path_rejection)?;

    let outcome = analysis::analyze_contract(&state, auth.user_id, id).await?;
    build_analysis_response(&state, outcome.contract).await
}

/// Shared builder for the analysis response used by both the `analyze`
/// and `get_analysis` handlers.
async fn build_analysis_response(
    state: &AppState,
    contract: Contract,
) -> AppResult<impl IntoResponse> {
    let risks = db::risks::list_risks_for_contract(state.db_pool(), contract.id)
        .await
        .map_err(AppError::Database)?;
    let obligations = db::obligations::list_obligations_for_contract(state.db_pool(), contract.id)
        .await
        .map_err(AppError::Database)?;

    let response = AnalysisResponse {
        contract_id: contract.id,
        analysis_status: contract.analysis_status,
        content_version: contract.content_version,
        analyzed_content_version: contract.analyzed_content_version,
        analyzed_at: contract.analyzed_at,
        analysis_provider: contract.analysis_provider,
        analysis_model: contract.analysis_model,
        analysis_prompt_version: contract.analysis_prompt_version,
        analysis_error: contract.analysis_error,
        start_date: contract.start_date,
        end_date: contract.end_date,
        risk_level: contract.risk_level,
        risk_score: contract.risk_score,
        risk_summary: contract.risk_summary,
        risks: risks
            .into_iter()
            .map(|r| RiskItem {
                id: r.id,
                title: r.title,
                description: r.description,
                risk_level: r.risk_level,
                risk_score: r.risk_score,
                evidence: r.evidence,
            })
            .collect(),
        obligations: obligations
            .into_iter()
            .map(|o| ObligationItem {
                id: o.id,
                title: o.title,
                description: o.description,
                due_date: o.due_date,
                responsible_party: o.responsible_party,
                status: o.status,
                risk_level: o.risk_level,
            })
            .collect(),
    };

    Ok((StatusCode::OK, Json(response)))
}

// -------------------------------------------------------------------------
// PDF extraction
// -------------------------------------------------------------------------

/// Extracts text from an uploaded PDF.
///
/// Body: `multipart/form-data` with a single `file` field containing a
/// PDF. Returns `{ "text": "..." }`.
///
/// Does **not** create a contract. The caller feeds the returned text
/// into the normal create flow, where it goes through the usual
/// validation (size, emptiness).
///
/// Rejects:
///   * missing `file` field
///   * files larger than [`EXTRACT_BODY_LIMIT_BYTES`]
///   * data whose magic bytes are not `%PDF-`
///   * encrypted PDFs
///   * scanned PDFs with no text layer
pub async fn extract_text(
    _auth: AuthenticatedUser,
    mut multipart: Multipart,
) -> AppResult<Json<ExtractTextResponse>> {
    let mut pdf_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::Validation("invalid multipart body".to_string()))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|_| AppError::Validation("failed to read upload".to_string()))?;
            if data.len() > crate::services::pdf::MAX_PDF_BYTES {
                return Err(AppError::PayloadTooLarge);
            }
            pdf_bytes = Some(data.to_vec());
        }
    }

    let bytes =
        pdf_bytes.ok_or_else(|| AppError::Validation("missing `file` field".to_string()))?;

    let text = crate::services::pdf::extract_text(&bytes).map_err(|e| {
        use crate::services::pdf::PdfError;
        match e {
            PdfError::Unreadable => AppError::Validation("file is not a valid PDF".to_string()),
            PdfError::Encrypted => AppError::Validation(
                "PDF is password-protected; please remove the password first".to_string(),
            ),
            PdfError::NoText => AppError::Validation(
                "PDF has no text layer (it may be a scanned document); \
                 copy the text manually instead"
                    .to_string(),
            ),
            PdfError::Other => AppError::Validation("could not extract text from PDF".to_string()),
        }
    })?;

    Ok(Json(ExtractTextResponse { text }))
}

// -------------------------------------------------------------------------
// Image OCR
// -------------------------------------------------------------------------

/// Extracts text from an uploaded image (JPEG, PNG, or WebP).
///
/// Body: `multipart/form-data` with a single `file` field. Returns
/// `{ "text": "..." }`.
///
/// Rejects:
///   * missing `file` field
///   * files larger than [`EXTRACT_BODY_LIMIT_BYTES`]
///   * non-image bytes (no JPEG/PNG/WebP magic)
///   * images with no readable text
///   * unconfigured AI provider
pub async fn extract_image(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    mut multipart: Multipart,
) -> AppResult<Json<ExtractTextResponse>> {
    let config = state.config();
    let api_key = config
        .ai_api_key
        .as_deref()
        .ok_or_else(|| AppError::ExternalService("AI provider is not configured".to_string()))?
        .to_string();

    let mut image_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::Validation("invalid multipart body".to_string()))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|_| AppError::Validation("failed to read upload".to_string()))?;
            if data.len() > crate::services::vision::MAX_IMAGE_BYTES {
                return Err(AppError::PayloadTooLarge);
            }
            image_bytes = Some(data.to_vec());
        }
    }
    let bytes =
        image_bytes.ok_or_else(|| AppError::Validation("missing `file` field".to_string()))?;

    // Build a dedicated client with a generous timeout: vision calls
    // are slower than text-only calls.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::ExternalService("could not build http client".to_string()))?;

    let text =
        crate::services::vision::extract_text(&client, &config.ai_base_url, &api_key, &bytes)
            .await
            .map_err(|e| {
                use crate::services::vision::VisionError;
                match e {
                    VisionError::UnsupportedFormat => AppError::Validation(
                        "image format not supported (use JPEG, PNG, or WebP)".to_string(),
                    ),
                    VisionError::TooLarge => AppError::PayloadTooLarge,
                    VisionError::NotConfigured => {
                        AppError::ExternalService("AI vision is not configured".to_string())
                    }
                    VisionError::NoText => AppError::Validation(
                        "no readable text found in the image; try a clearer photo".to_string(),
                    ),
                    VisionError::RequestFailed | VisionError::ProviderError => {
                        AppError::ExternalService("vision provider error".to_string())
                    }
                }
            })?;

    Ok(Json(ExtractTextResponse { text }))
}
