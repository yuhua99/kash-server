use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPayload {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub amount: f64,
    pub category_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecordPayload {
    pub name: String,
    pub amount: f64,
    pub category_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecordPayload {
    pub name: Option<String>,
    pub amount: Option<f64>,
    pub category_id: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecordsResponse {
    pub records: Vec<Record>,
    pub total_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub is_income: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCategoryPayload {
    pub name: String,
    pub is_income: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCategoryPayload {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCategoriesResponse {
    pub categories: Vec<Category>,
    pub total_count: u32,
    pub limit: u32,
    pub offset: u32,
}
