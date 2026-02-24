use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::constants::*;
use crate::models::{
    Category, CreateCategoryPayload, GetCategoriesQuery, GetCategoriesResponse,
    UpdateCategoryPayload,
};
use crate::utils::{
    db_error, db_error_with_context, validate_categories_limit, validate_offset,
    validate_string_length,
};
use crate::{AppState, Db, TransactionError, with_transaction};

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

pub async fn validate_category_not_in_use(
    db: &Db,
    user_id: &str,
    category_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = db.read().await;

    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE category_id = ? AND owner_user_id = ?",
            (category_id, user_id),
        )
        .await
        .map_err(|_| db_error_with_context("failed to check category usage"))?;

    if let Some(row) = rows.next().await.map_err(|_| db_error())? {
        let count: u32 = row.get(0).map_err(|_| db_error())?;
        if count > 0 {
            return Err((
                StatusCode::CONFLICT,
                "Cannot delete category: it has associated records".to_string(),
            ));
        }
    }

    Ok(())
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

    let conn = app_state.main_db.read().await;

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

    let conn = app_state.main_db.write().await;

    let mut existing_rows = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE id = ? AND owner_user_id = ?",
            (category_id.as_str(), user.id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query existing category"))?;

    let existing_category = if let Some(row) = existing_rows.next().await.map_err(|_| db_error())? {
        extract_category_from_row(row)?
    } else {
        return Err((StatusCode::NOT_FOUND, "Category not found".to_string()));
    };

    let mut conflict_rows = conn
        .query(
            "SELECT id FROM categories WHERE owner_user_id = ? AND LOWER(name) = LOWER(?) AND id != ?",
            (user.id.as_str(), category_name.as_str(), category_id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to check name conflict"))?;

    if conflict_rows
        .next()
        .await
        .map_err(|_| db_error())?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "Category name already exists (case-insensitive)".to_string(),
        ));
    }

    let affected_rows = conn
        .execute(
            "UPDATE categories SET name = ? WHERE id = ? AND owner_user_id = ?",
            (
                category_name.as_str(),
                category_id.as_str(),
                user.id.as_str(),
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to update category"))?;

    if affected_rows == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Category not found or no changes made".to_string(),
        ));
    }

    let updated_category = Category {
        id: category_id,
        name: category_name,
        is_income: existing_category.is_income,
    };

    Ok((StatusCode::OK, Json(updated_category)))
}

pub async fn delete_category(
    State(app_state): State<AppState>,
    session: Session,
    Path(category_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = get_current_user(&session).await?;

    {
        let conn = app_state.main_db.read().await;
        let mut existing_rows = conn
            .query(
                "SELECT id FROM categories WHERE id = ? AND owner_user_id = ?",
                (category_id.as_str(), user.id.as_str()),
            )
            .await
            .map_err(|_| db_error_with_context("failed to query existing category"))?;

        if existing_rows
            .next()
            .await
            .map_err(|_| db_error())?
            .is_none()
        {
            return Err((StatusCode::NOT_FOUND, "Category not found".to_string()));
        }

        validate_category_not_in_use(&app_state.main_db, &user.id, &category_id).await?;
    }

    let conn = app_state.main_db.write().await;
    let affected_rows = conn
        .execute(
            "DELETE FROM categories WHERE id = ? AND owner_user_id = ?",
            (category_id.as_str(), user.id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to delete category"))?;

    if affected_rows == 0 {
        return Err((StatusCode::NOT_FOUND, "Category not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}
