use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
}

#[derive(Deserialize)]
pub struct RegisterPayload {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub amount: f64,
    pub category_id: String,
    pub date: String,
}

#[derive(Deserialize)]
pub struct CreateRecordPayload {
    pub name: String,
    pub amount: f64,
    pub category_id: String,
    pub date: String,
}

#[derive(Deserialize)]
pub struct UpdateRecordPayload {
    pub name: Option<String>,
    pub amount: Option<f64>,
    pub category_id: Option<String>,
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub struct GetRecordsQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Serialize)]
pub struct GetRecordsResponse {
    pub records: Vec<Record>,
    pub total_count: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub is_income: bool,
}

#[derive(Deserialize)]
pub struct CreateCategoryPayload {
    pub name: String,
    pub is_income: bool,
}

#[derive(Deserialize)]
pub struct UpdateCategoryPayload {
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct GetCategoriesQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub search: Option<String>,
}

#[derive(Serialize)]
pub struct GetCategoriesResponse {
    pub categories: Vec<Category>,
    pub total_count: u32,
    pub limit: u32,
    pub offset: u32,
}
