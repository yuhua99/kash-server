use time::OffsetDateTime;
use uuid::Uuid;

use my_budget_server::DbPool;
use my_budget_server::categories::validate_category_name;
use my_budget_server::models::Record;
use my_budget_server::records;
use my_budget_server::utils::{get_user_database_from_pool, validate_date};

use crate::constants::PENDING_ACTION_TTL_SECONDS;
use crate::db::fetch_record_by_id;
use crate::helpers::{
    normalize_amount_by_category, resolve_category_id, resolve_record_id_by_name,
};
use crate::models::{
    AiEditResult, BotState, CategoryInfo, PendingAction, PendingActionType, PendingRecordPatch,
};

// ---------------------------------------------------------------------------
// Build pending actions from AI edit result
// ---------------------------------------------------------------------------

pub async fn build_pending_action_from_edit(
    db_pool: &DbPool,
    user_id: &str,
    edit: &AiEditResult,
    recent_records: &[Record],
    categories: &[CategoryInfo],
) -> Result<PendingAction, String> {
    match edit.target_type.trim().to_ascii_lowercase().as_str() {
        "record" => {
            build_pending_record_edit(db_pool, user_id, edit, recent_records, categories).await
        }
        "category" => build_pending_category_edit(user_id, edit, categories),
        _ => Err("Please specify whether you want to edit one record or one category.".to_string()),
    }
}

async fn build_pending_record_edit(
    db_pool: &DbPool,
    user_id: &str,
    edit: &AiEditResult,
    records: &[Record],
    categories: &[CategoryInfo],
) -> Result<PendingAction, String> {
    let trimmed_id = edit.target_id.trim();
    let existing = if !trimmed_id.is_empty() {
        fetch_record_by_id(db_pool, user_id, trimmed_id).await?
    } else {
        let record_id = resolve_record_id_by_name(records, &edit.target_name)?;
        records
            .iter()
            .find(|record| record.id == record_id)
            .cloned()
            .ok_or_else(|| "Record not found.".to_string())?
    };
    let record_id = existing.id.clone();

    let new_name = edit
        .new_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(name) = &new_name {
        records::validate_record_name(name).map_err(|(_, message)| message)?;
    }

    if let Some(amount) = edit.new_amount {
        records::validate_record_amount(amount).map_err(|(_, message)| message)?;
    }

    let new_date = edit
        .new_date
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(date) = &new_date {
        validate_date(date).map_err(|(_, message)| message)?;
    }

    let provided_category_id = edit.new_category_id.as_deref().unwrap_or("");
    let provided_category_name = edit.new_category_name.as_deref().unwrap_or("");
    let new_category_id =
        resolve_category_id(categories, provided_category_id, provided_category_name);
    if (!provided_category_id.trim().is_empty() || !provided_category_name.trim().is_empty())
        && new_category_id.is_none()
    {
        return Err("Category not found for record update.".to_string());
    }

    let patch = PendingRecordPatch {
        name: new_name.clone(),
        amount: edit.new_amount,
        category_id: new_category_id.clone(),
        date: new_date.clone(),
    };

    if patch.is_empty() {
        return Err(
            "No record field changes detected. Please specify at least one field to edit."
                .to_string(),
        );
    }

    let mut parts = Vec::new();
    if let Some(name) = &patch.name {
        parts.push(format!("name: '{}' -> '{}'", existing.name, name));
    }
    if let Some(amount) = patch.amount {
        parts.push(format!("amount: {} -> {}", existing.amount, amount));
    }
    if let Some(category_id) = &patch.category_id {
        let old_name = categories
            .iter()
            .find(|category| category.id == existing.category_id)
            .map(|category| category.name.as_str())
            .unwrap_or("Unknown");
        let new_name = categories
            .iter()
            .find(|category| category.id == *category_id)
            .map(|category| category.name.as_str())
            .unwrap_or("Unknown");
        parts.push(format!("category: '{}' -> '{}'", old_name, new_name));
    }
    if let Some(date) = &patch.date {
        parts.push(format!("date: {} -> {}", existing.date, date));
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    Ok(PendingAction {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        expires_at: now + PENDING_ACTION_TTL_SECONDS,
        summary: format!("record {}: {}", record_id, parts.join(", ")),
        action: PendingActionType::RecordEdit { record_id, patch },
    })
}

fn build_pending_category_edit(
    user_id: &str,
    edit: &AiEditResult,
    categories: &[CategoryInfo],
) -> Result<PendingAction, String> {
    let target_id = if edit.target_id.trim().is_empty() {
        edit.category_id.trim()
    } else {
        edit.target_id.trim()
    };
    let target_name = if edit.target_name.trim().is_empty() {
        edit.category_name.trim()
    } else {
        edit.target_name.trim()
    };

    let category_id = resolve_category_id(categories, target_id, target_name).ok_or_else(|| {
        "Category not found. Please include category id or exact name.".to_string()
    })?;
    let category = categories
        .iter()
        .find(|item| item.id == category_id)
        .ok_or_else(|| "Category not found.".to_string())?;
    let new_name = edit
        .new_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Category edit only supports updating name.".to_string())?;

    validate_category_name(&new_name).map_err(|(_, message)| message)?;
    if category.name == new_name {
        return Err("Category name is unchanged.".to_string());
    }
    if categories
        .iter()
        .any(|item| item.id != category.id && item.name.eq_ignore_ascii_case(&new_name))
    {
        return Err("Category name already exists (case-insensitive).".to_string());
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    Ok(PendingAction {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        expires_at: now + PENDING_ACTION_TTL_SECONDS,
        summary: format!(
            "category {}: name '{}' -> '{}'",
            category.id, category.name, new_name
        ),
        action: PendingActionType::CategoryEdit {
            category_id: category.id.clone(),
            new_name,
        },
    })
}

// ---------------------------------------------------------------------------
// Execute pending actions
// ---------------------------------------------------------------------------

pub async fn execute_pending_action(
    state: &BotState,
    pending: &PendingAction,
) -> Result<String, String> {
    if pending.expires_at <= OffsetDateTime::now_utc().unix_timestamp() {
        return Err(
            "This confirmation expired after 3 minutes. Please request the edit again.".to_string(),
        );
    }

    match &pending.action {
        PendingActionType::RecordEdit { record_id, patch } => {
            apply_record_edit(&state.db_pool, &pending.user_id, record_id, patch).await?;
            Ok(format!("Confirmed and updated {}.", pending.summary))
        }
        PendingActionType::CategoryEdit {
            category_id,
            new_name,
        } => {
            apply_category_name_edit(&state.db_pool, &pending.user_id, category_id, new_name)
                .await?;
            Ok(format!("Confirmed and updated {}.", pending.summary))
        }
    }
}

// ---------------------------------------------------------------------------
// Apply edits to database
// ---------------------------------------------------------------------------

async fn apply_record_edit(
    db_pool: &DbPool,
    user_id: &str,
    record_id: &str,
    patch: &PendingRecordPatch,
) -> Result<(), String> {
    if patch.is_empty() {
        return Err("No record fields provided for update.".to_string());
    }

    if let Some(name) = &patch.name {
        records::validate_record_name(name).map_err(|(_, message)| message)?;
    }
    if let Some(amount) = patch.amount {
        records::validate_record_amount(amount).map_err(|(_, message)| message)?;
    }
    if let Some(date) = &patch.date {
        validate_date(date).map_err(|(_, message)| message)?;
    }

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    if let Some(category_id) = &patch.category_id {
        my_budget_server::utils::validate_category_exists(&user_db, category_id)
            .await
            .map_err(|(_, message)| message)?;
    }

    let conn = user_db.write().await;
    let mut existing_rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
            [record_id],
        )
        .await
        .map_err(|_| "Failed to query existing record".to_string())?;

    let existing = if let Some(row) = existing_rows
        .next()
        .await
        .map_err(|_| "Failed to query existing record".to_string())?
    {
        records::extract_record_from_row(row).map_err(|(_, message)| message)?
    } else {
        return Err("Record not found.".to_string());
    };

    let updated_name = patch.name.as_deref().unwrap_or(&existing.name);
    let updated_category_id = patch
        .category_id
        .as_deref()
        .unwrap_or(&existing.category_id);
    let updated_amount = if let Some(amount) = patch.amount {
        let mut category_rows = conn
            .query(
                "SELECT is_income FROM categories WHERE id = ?",
                [updated_category_id],
            )
            .await
            .map_err(|_| "Failed to query category type".to_string())?;

        let is_income: bool = if let Some(row) = category_rows
            .next()
            .await
            .map_err(|_| "Failed to query category type".to_string())?
        {
            row.get(0)
                .map_err(|_| "Invalid category data".to_string())?
        } else {
            return Err("Category not found.".to_string());
        };

        normalize_amount_by_category(amount, is_income)
    } else {
        existing.amount
    };
    let updated_date = patch.date.as_deref().unwrap_or(&existing.date);

    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ?",
            (
                updated_name,
                updated_amount,
                updated_category_id,
                updated_date,
                record_id,
            ),
        )
        .await
        .map_err(|_| "Failed to update record".to_string())?;
    if affected_rows == 0 {
        return Err("Record not found or no changes made.".to_string());
    }

    Ok(())
}

async fn apply_category_name_edit(
    db_pool: &DbPool,
    user_id: &str,
    category_id: &str,
    new_name: &str,
) -> Result<(), String> {
    validate_category_name(new_name).map_err(|(_, message)| message)?;

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.write().await;

    let mut existing_rows = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE id = ?",
            [category_id],
        )
        .await
        .map_err(|_| "Failed to query existing category".to_string())?;
    if existing_rows
        .next()
        .await
        .map_err(|_| "Failed to query existing category".to_string())?
        .is_none()
    {
        return Err("Category not found.".to_string());
    }

    let mut conflict_rows = conn
        .query(
            "SELECT id FROM categories WHERE LOWER(name) = LOWER(?) AND id != ?",
            (new_name, category_id),
        )
        .await
        .map_err(|_| "Failed to check category name conflict".to_string())?;
    if conflict_rows
        .next()
        .await
        .map_err(|_| "Failed to check category name conflict".to_string())?
        .is_some()
    {
        return Err("Category name already exists (case-insensitive).".to_string());
    }

    let affected_rows = conn
        .execute(
            "UPDATE categories SET name = ? WHERE id = ?",
            (new_name, category_id),
        )
        .await
        .map_err(|_| "Failed to update category".to_string())?;
    if affected_rows == 0 {
        return Err("Category not found or no changes made.".to_string());
    }

    Ok(())
}
