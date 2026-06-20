use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::constants::*;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{
    Category, CreateCategoryPayload, GetCategoriesQuery, GetCategoriesResponse,
    UpdateCategoryPayload,
};
use crate::validation::{validate_categories_limit, validate_offset, validate_string_length};
use crate::{AppState, TransactionError, with_transaction};

pub fn validate_category_name(name: &str) -> Result<(), (StatusCode, String)> {
    validate_string_length(name, "Category name", MAX_CATEGORY_NAME_LENGTH)
}

pub fn extract_category_from_row(row: libsql::Row) -> Result<Category, (StatusCode, String)> {
    let id: String = row
        .get(0)
        .map_err(|_| db_error_with_context("invalid category data"))?;
    let name: String = row
        .get(1)
        .map_err(|_| db_error_with_context("invalid category data"))?;
    let is_income: bool = row
        .get(2)
        .map_err(|_| db_error_with_context("invalid category data"))?;

    Ok(Category {
        id,
        name,
        is_income,
    })
}

enum CreateCategoryError {
    Transaction(TransactionError),
    DbCheck,
    DbInsert,
    Conflict,
}

impl From<TransactionError> for CreateCategoryError {
    fn from(e: TransactionError) -> Self {
        CreateCategoryError::Transaction(e)
    }
}

impl From<CreateCategoryError> for (StatusCode, String) {
    fn from(e: CreateCategoryError) -> Self {
        match e {
            CreateCategoryError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            CreateCategoryError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            CreateCategoryError::DbCheck => {
                db_error_with_context("failed to check existing category")
            }
            CreateCategoryError::DbInsert => db_error_with_context("category creation failed"),
            CreateCategoryError::Conflict => (
                StatusCode::CONFLICT,
                "Category name already exists (case-insensitive)".to_string(),
            ),
        }
    }
}

#[utoipa::path(
    post,
    path = "/categories",
    tag = "categories",
    request_body = crate::models::CreateCategoryPayload,
    responses(
        (status = 201, description = "Category created", body = crate::models::Category),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "Conflict"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn create_category(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateCategoryPayload>,
) -> Result<(StatusCode, Json<Category>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    validate_category_name(&payload.name)?;
    let category_name = payload.name.trim().to_string();
    let is_income = payload.is_income;
    let db = &app_state.main_db;

    let category = with_transaction(db, |conn| {
        let name = category_name.clone();
        let owner_user_id = user.id.clone();
        Box::pin(async move {
            let mut existing_rows = conn
                .query(
                    "SELECT id FROM categories WHERE owner_user_id = ? AND LOWER(name) = LOWER(?)",
                    (owner_user_id.as_str(), name.as_str()),
                )
                .await
                .map_err(|_| CreateCategoryError::DbCheck)?;

            if existing_rows
                .next()
                .await
                .map_err(|_| CreateCategoryError::DbCheck)?
                .is_some()
            {
                return Err(CreateCategoryError::Conflict);
            }

            let category_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
                (
                    category_id.as_str(),
                    owner_user_id.as_str(),
                    name.as_str(),
                    is_income,
                ),
            )
            .await
            .map_err(|_| CreateCategoryError::DbInsert)?;

            Ok(Category {
                id: category_id,
                name,
                is_income,
            })
        })
    })
    .await
    .map_err(|e: CreateCategoryError| -> (StatusCode, String) { e.into() })?;

    Ok((StatusCode::CREATED, Json(category)))
}

#[utoipa::path(
    get,
    path = "/categories",
    tag = "categories",
    params(GetCategoriesQuery),
    responses(
        (status = 200, description = "Categories retrieved", body = crate::models::GetCategoriesResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn get_categories(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<GetCategoriesQuery>,
) -> Result<(StatusCode, Json<GetCategoriesResponse>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let limit = validate_categories_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;

    let search_term = query
        .search
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(search) = &search_term {
        validate_string_length(search, "Search term", MAX_SEARCH_TERM_LENGTH)?;
    }

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    let total_count: u32 = if let Some(search) = &search_term {
        let search_pattern = format!("%{}%", search);
        let mut count_rows = conn
            .query(
                "SELECT COUNT(*) FROM categories WHERE owner_user_id = ? AND name LIKE ? COLLATE NOCASE",
                (user.id.as_str(), search_pattern.as_str()),
            )
            .await
            .map_err(|_| db_error_with_context("failed to count categories"))?;

        if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
            row.get(0).map_err(|_| db_error())?
        } else {
            0
        }
    } else {
        let mut count_rows = conn
            .query(
                "SELECT COUNT(*) FROM categories WHERE owner_user_id = ?",
                [user.id.as_str()],
            )
            .await
            .map_err(|_| db_error_with_context("failed to count categories"))?;

        if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
            row.get(0).map_err(|_| db_error())?
        } else {
            0
        }
    };

    let mut rows = if let Some(search) = &search_term {
        let search_pattern = format!("%{}%", search);
        conn.query(
            "SELECT id, name, is_income FROM categories WHERE owner_user_id = ? AND name LIKE ? COLLATE NOCASE ORDER BY name ASC LIMIT ? OFFSET ?",
            (user.id.as_str(), search_pattern.as_str(), limit, offset),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query categories"))?
    } else {
        conn.query(
            "SELECT id, name, is_income FROM categories WHERE owner_user_id = ? ORDER BY name ASC LIMIT ? OFFSET ?",
            (user.id.as_str(), limit, offset),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query categories"))?
    };

    let mut categories = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| db_error())? {
        categories.push(extract_category_from_row(row)?);
    }

    Ok((
        StatusCode::OK,
        Json(GetCategoriesResponse {
            categories,
            total_count,
            limit,
            offset,
        }),
    ))
}

enum UpdateCategoryError {
    Transaction(TransactionError),
    NotFound(&'static str),
    Conflict,
    Db,
}

impl From<TransactionError> for UpdateCategoryError {
    fn from(e: TransactionError) -> Self {
        UpdateCategoryError::Transaction(e)
    }
}

impl From<UpdateCategoryError> for (StatusCode, String) {
    fn from(e: UpdateCategoryError) -> Self {
        match e {
            UpdateCategoryError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            UpdateCategoryError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            UpdateCategoryError::NotFound(message) => (StatusCode::NOT_FOUND, message.to_string()),
            UpdateCategoryError::Conflict => (
                StatusCode::CONFLICT,
                "Category name already exists (case-insensitive)".to_string(),
            ),
            UpdateCategoryError::Db => db_error(),
        }
    }
}

#[utoipa::path(
    put,
    path = "/categories/{id}",
    tag = "categories",
    params(("id" = String, Path, description = "category id")),
    request_body = crate::models::UpdateCategoryPayload,
    responses(
        (status = 200, description = "Category updated", body = crate::models::Category),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Conflict"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn update_category(
    State(app_state): State<AppState>,
    session: Session,
    Path(category_id): Path<String>,
    Json(payload): Json<UpdateCategoryPayload>,
) -> Result<(StatusCode, Json<Category>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let category_name = if let Some(ref name) = payload.name {
        validate_category_name(name)?;
        name.trim().to_string()
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Category name is required for update".to_string(),
        ));
    };

    let updated_category = with_transaction(&app_state.main_db, |conn| {
        let category_id = category_id.clone();
        let category_name = category_name.clone();
        let owner_user_id = user.id.clone();
        Box::pin(async move {
            let mut existing_rows = conn
                .query(
                    "SELECT id, name, is_income FROM categories WHERE id = ? AND owner_user_id = ?",
                    (category_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| UpdateCategoryError::Db)?;

            let existing_category = if let Some(row) = existing_rows
                .next()
                .await
                .map_err(|_| UpdateCategoryError::Db)?
            {
                extract_category_from_row(row).map_err(|_| UpdateCategoryError::Db)?
            } else {
                return Err(UpdateCategoryError::NotFound("Category not found"));
            };

            let mut conflict_rows = conn
                .query(
                    "SELECT id FROM categories WHERE owner_user_id = ? AND LOWER(name) = LOWER(?) AND id != ?",
                    (
                        owner_user_id.as_str(),
                        category_name.as_str(),
                        category_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| UpdateCategoryError::Db)?;

            if conflict_rows
                .next()
                .await
                .map_err(|_| UpdateCategoryError::Db)?
                .is_some()
            {
                return Err(UpdateCategoryError::Conflict);
            }

            let affected_rows = conn
                .execute(
                    "UPDATE categories SET name = ? WHERE id = ? AND owner_user_id = ?",
                    (
                        category_name.as_str(),
                        category_id.as_str(),
                        owner_user_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| UpdateCategoryError::Db)?;

            if affected_rows == 0 {
                return Err(UpdateCategoryError::NotFound(
                    "Category not found or no changes made",
                ));
            }

            Ok(Category {
                id: category_id,
                name: category_name,
                is_income: existing_category.is_income,
            })
        })
    })
    .await
    .map_err(|e: UpdateCategoryError| -> (StatusCode, String) { e.into() })?;

    Ok((StatusCode::OK, Json(updated_category)))
}

enum DeleteCategoryError {
    Transaction(TransactionError),
    NotFound,
    Conflict,
    Db,
}

impl From<TransactionError> for DeleteCategoryError {
    fn from(e: TransactionError) -> Self {
        DeleteCategoryError::Transaction(e)
    }
}

impl From<DeleteCategoryError> for (StatusCode, String) {
    fn from(e: DeleteCategoryError) -> Self {
        match e {
            DeleteCategoryError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            DeleteCategoryError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            DeleteCategoryError::NotFound => {
                (StatusCode::NOT_FOUND, "Category not found".to_string())
            }
            DeleteCategoryError::Conflict => (
                StatusCode::CONFLICT,
                "Cannot delete category: it has associated records".to_string(),
            ),
            DeleteCategoryError::Db => db_error(),
        }
    }
}

#[utoipa::path(
    delete,
    path = "/categories/{id}",
    tag = "categories",
    params(("id" = String, Path, description = "category id")),
    responses(
        (status = 204, description = "Category deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Conflict"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn delete_category(
    State(app_state): State<AppState>,
    session: Session,
    Path(category_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = get_current_user(&session).await?;

    with_transaction(&app_state.main_db, |conn| {
        let category_id = category_id.clone();
        let owner_user_id = user.id.clone();
        Box::pin(async move {
            let mut existing_rows = conn
                .query(
                    "SELECT id FROM categories WHERE id = ? AND owner_user_id = ?",
                    (category_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| DeleteCategoryError::Db)?;

            if existing_rows
                .next()
                .await
                .map_err(|_| DeleteCategoryError::Db)?
                .is_none()
            {
                return Err(DeleteCategoryError::NotFound);
            }
            drop(existing_rows);

            let mut usage_rows = conn
                .query(
                    "SELECT COUNT(*) FROM records WHERE category_id = ? AND owner_user_id = ?",
                    (category_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| DeleteCategoryError::Db)?;

            if let Some(row) = usage_rows
                .next()
                .await
                .map_err(|_| DeleteCategoryError::Db)?
            {
                let count: u32 = row.get(0).map_err(|_| DeleteCategoryError::Db)?;
                if count > 0 {
                    return Err(DeleteCategoryError::Conflict);
                }
            }
            drop(usage_rows);

            let affected_rows = conn
                .execute(
                    "DELETE FROM categories WHERE id = ? AND owner_user_id = ?",
                    (category_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| DeleteCategoryError::Db)?;

            if affected_rows == 0 {
                return Err(DeleteCategoryError::NotFound);
            }

            Ok(())
        })
    })
    .await
    .map_err(|e: DeleteCategoryError| -> (StatusCode, String) { e.into() })?;

    Ok(StatusCode::NO_CONTENT)
}
