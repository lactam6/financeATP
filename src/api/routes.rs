//! API Routes
//!
//! HTTP endpoint definitions.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::OperationContext;
use crate::error::AppError;
use crate::handlers::{
    CreateUserCommand, CreateUserHandler, MintCommand, MintHandler, TransferCommand,
    TransferHandler, UpdateUserCommand, UpdateUserHandler, DeactivateUserCommand, DeactivateUserHandler,
};
use crate::projection::ProjectionService;

use super::middleware::{AuthenticatedApiKey, RequestUser};

// =========================================================================
// Request/Response types
// =========================================================================

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateUserRequest {
    pub user_id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub user_id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub balance: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub is_system: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TransferRequest {
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub amount: String,
    #[serde(default)]
    pub memo: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub transfer_id: Uuid,
    pub status: String,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct TransferDetailResponse {
    pub id: Uuid,
    pub from_account_id: Uuid,
    pub to_account_id: Uuid,
    pub amount: Decimal,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MintRequest {
    pub recipient_user_id: Uuid,
    pub amount: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct MintResponse {
    pub mint_id: Uuid,
    pub status: String,
    pub to_user_id: Uuid,
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct BurnRequest {
    pub from_user_id: Uuid,
    pub amount: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct BurnResponse {
    pub burn_id: Uuid,
    pub status: String,
    pub from_user_id: Uuid,
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct BalanceQuery {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub user_id: Uuid,
    pub balance: Decimal,
}

#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub event_id: Uuid,
    pub event_type: String,
    pub amount: Option<Decimal>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub user_id: Uuid,
    pub entries: Vec<HistoryEntry>,
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub aggregate_type: Option<String>,
    #[serde(default)]
    pub aggregate_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct EventResponse {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct EventsListResponse {
    pub events: Vec<EventResponse>,
    pub total: i64,
}

// =========================================================================
// API Key Management Types
// =========================================================================

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub permissions: Vec<String>,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: i32,
}

fn default_rate_limit() -> i32 {
    1000
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub api_key: String,  // Only returned on creation
    pub key_prefix: String,
    pub permissions: Vec<String>,
    pub rate_limit_per_minute: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub permissions: Vec<String>,
    pub rate_limit_per_minute: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub rate_limit_per_minute: Option<i32>,
    pub is_active: Option<bool>,
}

// =========================================================================
// API Router
// =========================================================================

/// Create the API router
pub fn create_router() -> Router<PgPool> {
    Router::new()
        // M120: User endpoints
        .route("/users", post(create_user))
        // M121, M122, M123: User CRUD
        .route("/users/:user_id", get(get_user))
        .route("/users/:user_id", patch(update_user))
        .route("/users/:user_id", delete(delete_user))
        // M124: Balance
        .route("/users/:user_id/balance", get(get_user_balance))
        // M125: History
        .route("/users/:user_id/history", get(get_user_history))
        // M126, M127: Transfers
        .route("/transfers", post(transfer))
        .route("/transfers/:transfer_id", get(get_transfer))
        // M128, M129, M130: Admin
        .route("/admin/mint", post(mint))
        .route("/admin/burn", post(burn))
        .route("/admin/events", get(get_events))
        // API Key Management
        .route("/admin/api-keys", post(create_api_key))
        .route("/admin/api-keys", get(list_api_keys))
        .route("/admin/api-keys/:key_id", patch(update_api_key))
        .route("/admin/api-keys/:key_id", delete(delete_api_key))
        // Legacy endpoints for compatibility
        .route("/transfer", post(transfer))
        .route("/mint", post(mint))
        .route("/balance", get(get_balance_legacy))
        .route("/balance/:user_id", get(get_balance_by_path))
}

// =========================================================================
// M120: POST /users
// =========================================================================

/// Create a new user
async fn create_user(
    State(pool): State<PgPool>,
    Extension(context): Extension<OperationContext>,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<CreateUserResponse>), AppError> {
    let handler = CreateUserHandler::new(pool);

    let email = request.email.clone();
    let display_name = request.display_name.clone();
    let command = CreateUserCommand::new(request.user_id, request.username, email.clone());
    let command = if let Some(ref dn) = display_name {
        command.with_display_name(dn.clone())
    } else {
        command
    };

    let result = handler.execute(command, &context).await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateUserResponse {
            user_id: result.user_id,
            username: result.username,
            email,
            display_name,
            balance: "0.00000000".to_string(),
            created_at: chrono::Utc::now(),
        }),
    ))
}

// =========================================================================
// M121: GET /users/:user_id
// =========================================================================

/// Get user by ID
async fn get_user(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserResponse>, AppError> {
    let user: Option<(Uuid, String, String, Option<String>, bool, bool, DateTime<Utc>, DateTime<Utc>)> =
        sqlx::query_as(
            r#"
            SELECT id, username, email, display_name, is_system, is_active, created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&pool)
        .await?;

    let (id, username, email, display_name, is_system, is_active, created_at, updated_at) =
        user.ok_or_else(|| AppError::UserNotFound(user_id.to_string()))?;

    Ok(Json(UserResponse {
        id,
        username,
        email,
        display_name,
        is_system,
        is_active,
        created_at,
        updated_at,
    }))
}

// =========================================================================
// M122: PATCH /users/:user_id
// =========================================================================

/// Update user
async fn update_user(
    State(pool): State<PgPool>,
    Extension(context): Extension<OperationContext>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    Path(user_id): Path<Uuid>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, AppError> {
    // Check permission
    if !api_key.has_permission("write:users") {
        return Err(AppError::Forbidden("write:users permission required".to_string()));
    }

    // Check if user is system user
    let is_system: Option<bool> = sqlx::query_scalar("SELECT is_system FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await?;

    let is_system = is_system.ok_or_else(|| AppError::UserNotFound(user_id.to_string()))?;

    if is_system {
        return Err(AppError::Forbidden("Cannot modify system user".to_string()));
    }

    // Build changes
    let changes = crate::domain::UserChanges {
        display_name: request.display_name.clone(),
        email: request.email.clone(),
    };

    // Execute via handler (event sourced)
    let handler = UpdateUserHandler::new(pool.clone());
    let command = UpdateUserCommand::new(user_id, changes);
    handler.execute(command, &context).await?;

    // Return updated user
    get_user(State(pool), Path(user_id)).await
}

// =========================================================================
// M123: DELETE /users/:user_id
// =========================================================================

/// Deactivate user (soft delete)
async fn delete_user(
    State(pool): State<PgPool>,
    Extension(context): Extension<OperationContext>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    // Check permission
    if !api_key.has_permission("write:users") {
        return Err(AppError::Forbidden("write:users permission required".to_string()));
    }

    // Execute via handler (event sourced)
    let handler = DeactivateUserHandler::new(pool);
    let command = DeactivateUserCommand::new(user_id);
    handler.execute(command, &context).await?;

    Ok(StatusCode::NO_CONTENT)
}

// =========================================================================
// M124: GET /users/:user_id/balance
// =========================================================================

/// Get user balance
async fn get_user_balance(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, AppError> {
    let projection = ProjectionService::new(pool);

    let balance = projection
        .get_user_balance(user_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::UserNotFound(user_id.to_string()))?;

    Ok(Json(BalanceResponse { user_id, balance }))
}

// =========================================================================
// M125: GET /users/:user_id/history
// =========================================================================

/// Get user transaction history
async fn get_user_history(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<HistoryResponse>, AppError> {
    // Get user's account
    let account_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = $1 AND account_type = 'user_wallet'",
    )
    .bind(user_id)
    .fetch_optional(&pool)
    .await?;

    let account_id = account_id.ok_or_else(|| AppError::UserNotFound(user_id.to_string()))?;

    // Get events for this account
    let events: Vec<(Uuid, String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT id, event_type, event_data, created_at
        FROM events
        WHERE aggregate_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(account_id)
    .fetch_all(&pool)
    .await?;

    let entries: Vec<HistoryEntry> = events
        .into_iter()
        .map(|(id, event_type, data, created_at)| {
            let amount = data.get("amount").and_then(|v| {
                v.as_str()
                    .and_then(|s| s.parse::<Decimal>().ok())
                    .or_else(|| v.as_f64().map(|f| Decimal::from_f64_retain(f).unwrap_or_default()))
            });
            let description = data
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            HistoryEntry {
                event_id: id,
                event_type,
                amount,
                description,
                created_at,
            }
        })
        .collect();

    Ok(Json(HistoryResponse {
        user_id,
        entries,
    }))
}

// =========================================================================
// M126: POST /transfers
// =========================================================================

/// Transfer ATP between users
async fn transfer(
    State(pool): State<PgPool>,
    Extension(context): Extension<OperationContext>,
    request_user: Option<Extension<RequestUser>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<TransferRequest>,
) -> Result<Json<TransferResponse>, AppError> {
    // X-Request-User-Id is required for transfer
    let request_user = request_user
        .ok_or_else(|| AppError::MissingHeader("X-Request-User-Id".to_string()))?;

    // Build context with request user
    let context = context.with_request_user(request_user.user_id);

    // Extract idempotency key if present
    let idempotency_key = headers.get("Idempotency-Key");
    let idem_key = idempotency_key
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok());

    let handler = TransferHandler::new(pool);

    let command = TransferCommand::new(request.from_user_id, request.to_user_id, request.amount);
    let command = if let Some(memo) = request.memo {
        command.with_memo(memo)
    } else {
        command
    };

    let result = handler.execute(command, idem_key, &context).await?;

    Ok(Json(TransferResponse {
        transfer_id: result.transfer_id,
        status: result.status,
        from_user_id: result.from_user_id,
        to_user_id: result.to_user_id,
        amount: result.amount,
        created_at: chrono::Utc::now(),
    }))
}

// =========================================================================
// M127: GET /transfers/:transfer_id
// =========================================================================

/// Get transfer details
async fn get_transfer(
    State(pool): State<PgPool>,
    Path(transfer_id): Path<Uuid>,
) -> Result<Json<TransferDetailResponse>, AppError> {
    // Find the debit event with this transfer_id
    let transfer: Option<(Uuid, Uuid, Decimal, String, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT 
            le.journal_id,
            le.account_id,
            le.amount,
            COALESCE(le.description, '') as description,
            le.created_at
        FROM ledger_entries le
        WHERE le.journal_id = $1 AND le.entry_type = 'debit'
        LIMIT 1
        "#,
    )
    .bind(transfer_id)
    .fetch_optional(&pool)
    .await?;

    let (journal_id, from_account_id, amount, description, created_at) = transfer
        .ok_or_else(|| AppError::InvalidRequest(format!("Transfer {} not found", transfer_id)))?;

    // Get the credit side
    let to_account_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT account_id FROM ledger_entries WHERE journal_id = $1 AND entry_type = 'credit' LIMIT 1",
    )
    .bind(journal_id)
    .fetch_optional(&pool)
    .await?;

    let to_account_id = to_account_id
        .ok_or_else(|| AppError::Internal("Invalid transfer: missing credit entry".to_string()))?;

    Ok(Json(TransferDetailResponse {
        id: journal_id,
        from_account_id,
        to_account_id,
        amount,
        description,
        created_at,
    }))
}

// =========================================================================
// M128: POST /admin/mint
// =========================================================================

/// Mint new ATP (admin only)
async fn mint(
    State(pool): State<PgPool>,
    Extension(context): Extension<OperationContext>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    headers: axum::http::HeaderMap,
    Json(request): Json<MintRequest>,
) -> Result<(StatusCode, Json<MintResponse>), AppError> {
    // Check admin permission
    if !api_key.has_permission("admin:mint") {
        return Err(AppError::Forbidden("admin:mint permission required".to_string()));
    }

    let idempotency_key = headers.get("Idempotency-Key");
    let idem_key = idempotency_key
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok());

    let handler = MintHandler::new(pool);

    let command = MintCommand::new(request.recipient_user_id, request.amount, request.reason);

    let result = handler.execute(command, idem_key, &context).await?;

    Ok((
        StatusCode::CREATED,
        Json(MintResponse {
            mint_id: result.mint_id,
            status: "completed".to_string(),
            to_user_id: result.recipient_user_id,
            amount: result.amount,
            created_at: chrono::Utc::now(),
        }),
    ))
}

// =========================================================================
// M129: POST /admin/burn
// =========================================================================

/// Burn ATP (admin only) - removes ATP from circulation
async fn burn(
    State(pool): State<PgPool>,
    Extension(context): Extension<OperationContext>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    headers: axum::http::HeaderMap,
    Json(request): Json<BurnRequest>,
) -> Result<(StatusCode, Json<BurnResponse>), AppError> {
    // Check admin permission
    if !api_key.has_permission("admin:burn") {
        return Err(AppError::Forbidden("admin:burn permission required".to_string()));
    }

    let idempotency_key = headers.get("Idempotency-Key");
    let idem_key = idempotency_key
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok());

    let handler = crate::handlers::BurnHandler::new(pool);

    let command = crate::handlers::BurnCommand::new(
        request.from_user_id,
        request.amount,
        request.reason,
    );

    let result = handler.execute(command, idem_key, &context).await?;

    Ok((
        StatusCode::CREATED,
        Json(BurnResponse {
            burn_id: result.burn_id,
            status: "completed".to_string(),
            from_user_id: result.from_user_id,
            amount: result.amount,
            created_at: chrono::Utc::now(),
        }),
    ))
}

// =========================================================================
// M130: GET /admin/events
// =========================================================================

/// Get events (admin only)
async fn get_events(
    State(pool): State<PgPool>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<EventsListResponse>, AppError> {
    // Check admin permission
    if !api_key.has_permission("admin:events") {
        return Err(AppError::Forbidden("admin:events permission required".to_string()));
    }

    let limit = query.limit.min(1000);
    let offset = query.offset;

    // Build query based on filters
    let events: Vec<(Uuid, String, Uuid, String, i64, DateTime<Utc>)> = if let Some(ref agg_type) = query.aggregate_type {
        if let Some(agg_id) = query.aggregate_id {
            sqlx::query_as(
                r#"
                SELECT id, aggregate_type, aggregate_id, event_type, version, created_at
                FROM events
                WHERE aggregate_type = $1 AND aggregate_id = $2
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(agg_type)
            .bind(agg_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, aggregate_type, aggregate_id, event_type, version, created_at
                FROM events
                WHERE aggregate_type = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(agg_type)
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?
        }
    } else {
        sqlx::query_as(
            r#"
            SELECT id, aggregate_type, aggregate_id, event_type, version, created_at
            FROM events
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await?
    };

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&pool)
        .await?;

    let events: Vec<EventResponse> = events
        .into_iter()
        .map(|(id, aggregate_type, aggregate_id, event_type, version, created_at)| {
            EventResponse {
                id,
                aggregate_type,
                aggregate_id,
                event_type,
                version,
                created_at,
            }
        })
        .collect();

    Ok(Json(EventsListResponse { events, total }))
}

// =========================================================================
// Legacy endpoints
// =========================================================================

/// Get user balance by query parameter (legacy)
async fn get_balance_legacy(
    State(pool): State<PgPool>,
    Query(query): Query<BalanceQuery>,
) -> Result<Json<BalanceResponse>, AppError> {
    get_user_balance(State(pool), Path(query.user_id)).await
}

/// Get user balance by path parameter (legacy)
async fn get_balance_by_path(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, AppError> {
    get_user_balance(State(pool), Path(user_id)).await
}

// =========================================================================
// API Key Management Handlers
// =========================================================================

/// Generate a random API key
fn generate_api_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 24] = rng.gen();
    format!("sk_live_{}", hex::encode(random_bytes))
}

/// Create a new API key
async fn create_api_key(
    State(pool): State<PgPool>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), AppError> {
    // Check for admin:api-keys permission
    if !api_key.permissions.iter().any(|p| p == "admin:api-keys") {
        return Err(AppError::Forbidden("admin:api-keys permission required".to_string()));
    }

    let id = Uuid::new_v4();
    let raw_key = generate_api_key();
    let key_prefix = raw_key[..8].to_string();
    let key_hash = format!("{:x}", sha2::Sha256::digest(raw_key.as_bytes()));
    let now = chrono::Utc::now();

    sqlx::query(
        r#"
        INSERT INTO api_keys (id, name, key_prefix, key_hash, permissions, rate_limit_per_minute, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#
    )
    .bind(id)
    .bind(&request.name)
    .bind(&key_prefix)
    .bind(&key_hash)
    .bind(&request.permissions)
    .bind(request.rate_limit_per_minute)
    .bind(now)
    .execute(&pool)
    .await?;

    Ok((StatusCode::CREATED, Json(CreateApiKeyResponse {
        id,
        name: request.name,
        api_key: raw_key,
        key_prefix,
        permissions: request.permissions,
        rate_limit_per_minute: request.rate_limit_per_minute,
        created_at: now,
    })))
}

/// List all API keys
async fn list_api_keys(
    State(pool): State<PgPool>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
) -> Result<Json<Vec<ApiKeyResponse>>, AppError> {
    // Check for admin:api-keys permission
    if !api_key.permissions.iter().any(|p| p == "admin:api-keys") {
        return Err(AppError::Forbidden("admin:api-keys permission required".to_string()));
    }

    let keys: Vec<ApiKeyResponse> = sqlx::query_as::<_, (Uuid, String, String, Vec<String>, i32, bool, DateTime<Utc>, Option<DateTime<Utc>>)>(
        r#"
        SELECT id, name, key_prefix, permissions, rate_limit_per_minute, is_active, created_at, last_used_at
        FROM api_keys
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|(id, name, key_prefix, permissions, rate_limit_per_minute, is_active, created_at, last_used_at)| {
        ApiKeyResponse {
            id,
            name,
            key_prefix,
            permissions,
            rate_limit_per_minute,
            is_active,
            created_at,
            last_used_at,
        }
    })
    .collect();

    Ok(Json(keys))
}

/// Update an API key
async fn update_api_key(
    State(pool): State<PgPool>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    Path(key_id): Path<Uuid>,
    Json(request): Json<UpdateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>, AppError> {
    // Check for admin:api-keys permission
    if !api_key.permissions.iter().any(|p| p == "admin:api-keys") {
        return Err(AppError::Forbidden("admin:api-keys permission required".to_string()));
    }

    // Build dynamic update query
    let mut updates = Vec::new();
    let mut params: Vec<String> = Vec::new();
    
    if let Some(ref name) = request.name {
        updates.push(format!("name = ${}", params.len() + 2));
        params.push(name.clone());
    }
    if let Some(ref rate_limit) = request.rate_limit_per_minute {
        updates.push(format!("rate_limit_per_minute = ${}", params.len() + 2));
        params.push(rate_limit.to_string());
    }
    if let Some(ref is_active) = request.is_active {
        updates.push(format!("is_active = ${}", params.len() + 2));
        params.push(is_active.to_string());
    }

    if updates.is_empty() && request.permissions.is_none() {
        return Err(AppError::InvalidRequest("No fields to update".to_string()));
    }

    // Handle permissions separately due to array type
    if let Some(ref permissions) = request.permissions {
        sqlx::query("UPDATE api_keys SET permissions = $2 WHERE id = $1")
            .bind(key_id)
            .bind(permissions)
            .execute(&pool)
            .await?;
    }

    // Handle other updates
    if let Some(ref name) = request.name {
        sqlx::query("UPDATE api_keys SET name = $2 WHERE id = $1")
            .bind(key_id)
            .bind(name)
            .execute(&pool)
            .await?;
    }
    if let Some(rate_limit) = request.rate_limit_per_minute {
        sqlx::query("UPDATE api_keys SET rate_limit_per_minute = $2 WHERE id = $1")
            .bind(key_id)
            .bind(rate_limit)
            .execute(&pool)
            .await?;
    }
    if let Some(is_active) = request.is_active {
        sqlx::query("UPDATE api_keys SET is_active = $2 WHERE id = $1")
            .bind(key_id)
            .bind(is_active)
            .execute(&pool)
            .await?;
    }

    // Fetch updated key
    let row: Option<(Uuid, String, String, Vec<String>, i32, bool, DateTime<Utc>, Option<DateTime<Utc>>)> = 
        sqlx::query_as(
            "SELECT id, name, key_prefix, permissions, rate_limit_per_minute, is_active, created_at, last_used_at FROM api_keys WHERE id = $1"
        )
        .bind(key_id)
        .fetch_optional(&pool)
        .await?;

    let (id, name, key_prefix, permissions, rate_limit_per_minute, is_active, created_at, last_used_at) = 
        row.ok_or_else(|| AppError::InvalidRequest("API key not found".to_string()))?;

    Ok(Json(ApiKeyResponse {
        id,
        name,
        key_prefix,
        permissions,
        rate_limit_per_minute,
        is_active,
        created_at,
        last_used_at,
    }))
}

/// Delete (deactivate) an API key
async fn delete_api_key(
    State(pool): State<PgPool>,
    Extension(api_key): Extension<AuthenticatedApiKey>,
    Path(key_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    // Check for admin:api-keys permission
    if !api_key.permissions.iter().any(|p| p == "admin:api-keys") {
        return Err(AppError::Forbidden("admin:api-keys permission required".to_string()));
    }

    // Soft delete by setting is_active = false
    let result = sqlx::query("UPDATE api_keys SET is_active = false WHERE id = $1")
        .bind(key_id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::InvalidRequest("API key not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_user_request_deserialize() {
        let json = r#"{
            "user_id": "550e8400-e29b-41d4-a716-446655440000",
            "username": "alice",
            "email": "alice@example.com"
        }"#;

        let request: CreateUserRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.username, "alice");
        assert!(request.display_name.is_none());
    }

    #[test]
    fn test_transfer_request_deserialize() {
        let json = r#"{
            "from_user_id": "550e8400-e29b-41d4-a716-446655440001",
            "to_user_id": "550e8400-e29b-41d4-a716-446655440002",
            "amount": "100.50",
            "memo": "Test payment"
        }"#;

        let request: TransferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.amount, "100.50");
        assert_eq!(request.memo, Some("Test payment".to_string()));
    }

    #[test]
    fn test_events_query_defaults() {
        let query: EventsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 0);
        assert!(query.aggregate_type.is_none());
    }
}
