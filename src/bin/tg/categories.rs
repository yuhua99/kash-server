use libsql::Connection;
use uuid::Uuid;

use kash_server::Db;
use kash_server::categories::validate_category_name;

use crate::models::CategoryInfo;

// ---------------------------------------------------------------------------
// Category helpers
// ---------------------------------------------------------------------------

pub(crate) fn resolve_category_id(
    categories: &[CategoryInfo],
    category_id: &str,
    category_name: &str,
) -> Option<String> {
    if !category_id.trim().is_empty() && categories.iter().any(|c| c.id == category_id) {
        return Some(category_id.to_string());
    }

    if !category_name.trim().is_empty()
        && let Some(category) = categories
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(category_name.trim()))
    {
        return Some(category.id.clone());
    }

    None
}

pub(crate) async fn load_categories(db: &Db, user_id: &str) -> Result<Vec<CategoryInfo>, String> {
    let conn = db
        .connect()
        .map_err(|_| "Failed to connect to database".to_string())?;
    let mut rows = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE owner_user_id = ? ORDER BY name ASC",
            [user_id],
        )
        .await
        .map_err(|_| "Failed to query categories".to_string())?;

    let mut categories = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query categories".to_string())?
    {
        let id: String = row.get(0).map_err(|_| "Invalid category".to_string())?;
        let name: String = row.get(1).map_err(|_| "Invalid category".to_string())?;
        let is_income: bool = row.get(2).map_err(|_| "Invalid category".to_string())?;
        categories.push(CategoryInfo {
            id,
            name,
            is_income,
        });
    }

    Ok(categories)
}

pub(crate) async fn get_or_create_category(
    db: &Db,
    user_id: &str,
    name: &str,
    is_income: bool,
) -> Result<CategoryInfo, String> {
    let trimmed = name.trim();
    let fallback = if trimmed.is_empty() { "Other" } else { trimmed };
    validate_category_name(fallback).map_err(|(_, message)| message)?;

    let conn = db
        .connect()
        .map_err(|_| "Failed to connect to database".to_string())?;

    let mut existing = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE owner_user_id = ? AND LOWER(name) = LOWER(?)",
            (user_id, fallback),
        )
        .await
        .map_err(|_| "Failed to query categories".to_string())?;

    if let Some(row) = existing
        .next()
        .await
        .map_err(|_| "Failed to query categories".to_string())?
    {
        let id: String = row.get(0).map_err(|_| "Invalid category".to_string())?;
        let name: String = row.get(1).map_err(|_| "Invalid category".to_string())?;
        let is_income: bool = row.get(2).map_err(|_| "Invalid category".to_string())?;
        return Ok(CategoryInfo {
            id,
            name,
            is_income,
        });
    }

    let category_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
        (category_id.as_str(), user_id, fallback, is_income),
    )
    .await
    .map_err(|_| "Failed to create category".to_string())?;

    Ok(CategoryInfo {
        id: category_id,
        name: fallback.to_string(),
        is_income,
    })
}

pub(crate) fn resolve_category_filter_id(
    categories: &[CategoryInfo],
    category_id: Option<&str>,
    category_name: Option<&str>,
) -> Result<String, String> {
    let provided_id = category_id.unwrap_or("").trim();
    let provided_name = category_name.unwrap_or("").trim();

    if provided_id.is_empty() && provided_name.is_empty() {
        return Ok(String::new());
    }

    resolve_category_id(categories, provided_id, provided_name).ok_or_else(|| {
        format!(
            "Category not found. Available categories: {}",
            format_category_options(categories)
        )
    })
}

pub(crate) async fn resolve_or_create_category(
    db: &Db,
    user_id: &str,
    categories: &[CategoryInfo],
    category_id: Option<&str>,
    category_name: Option<&str>,
    is_income: Option<bool>,
) -> Result<CategoryInfo, String> {
    let provided_id = category_id.unwrap_or("").trim();
    let provided_name = category_name.unwrap_or("").trim();

    if let Some(resolved_id) = resolve_category_id(categories, provided_id, provided_name)
        && let Some(category) = categories
            .iter()
            .find(|category| category.id == resolved_id)
    {
        return Ok(category.clone());
    }

    if !provided_name.is_empty()
        && let Some(income_flag) = is_income
    {
        return get_or_create_category(db, user_id, provided_name, income_flag).await;
    }

    if categories.is_empty() {
        return Err(
            "No categories found. Please provide category_name and is_income when creating the first record."
                .to_string(),
        );
    }

    Err(format!(
        "Category not found. Available categories: {}",
        format_category_options(categories)
    ))
}

pub(crate) fn format_category_options(categories: &[CategoryInfo]) -> String {
    if categories.is_empty() {
        return "(none)".to_string();
    }

    categories
        .iter()
        .map(|category| format!("{} ({})", category.name, category.id))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) async fn get_category_is_income(
    conn: &Connection,
    user_id: &str,
    category_id: &str,
) -> Result<bool, String> {
    let mut rows = conn
        .query(
            "SELECT is_income FROM categories WHERE id = ? AND owner_user_id = ?",
            (category_id, user_id),
        )
        .await
        .map_err(|_| "Failed to query category type".to_string())?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query category type".to_string())?
    {
        row.get(0).map_err(|_| "Invalid category data".to_string())
    } else {
        Err("Category does not exist".to_string())
    }
}
