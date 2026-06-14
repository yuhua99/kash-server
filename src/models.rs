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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserSettings {
    pub main_currency: String,
}

#[derive(Deserialize)]
pub struct UpdateUserSettingsPayload {
    pub main_currency: String,
}

#[derive(Deserialize)]
pub struct GetFxRatesQuery {
    pub from: String,
    pub to: String,
    pub quotes: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExchangeRateRow {
    pub date: String,
    pub currency: String,
    // 1 USD = rate * currency
    pub rate: f64,
}

#[derive(Serialize)]
pub struct GetFxRatesResponse {
    pub rates: Vec<ExchangeRateRow>,
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
    pub currency: String,
    pub category_id: Option<String>,
    pub date: String,
}

#[derive(Deserialize)]
pub struct CreateRecordPayload {
    pub name: String,
    pub amount: f64,
    pub currency: String,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SendFriendRequestPayload {
    pub friend_username: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AcceptFriendPayload {
    pub friend_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateNicknamePayload {
    pub friend_id: String,
    pub nickname: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoveFriendPayload {
    pub friend_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FriendshipRelation {
    pub id: String,
    pub user_id: String,
    pub pending: bool,
    pub nickname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SplitParticipant {
    pub user_id: String,
    pub amount: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateSplitPayload {
    pub idempotency_key: String,
    pub total_amount: f64,
    pub currency: String,
    pub description: String,
    pub date: String,
    pub category_id: String,
    pub splits: Vec<SplitParticipant>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ParticipantBrief {
    pub id: String,
    pub debtor_user_id: String,
    pub amount: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SplitCreatedResponse {
    pub split_id: String,
    pub creditor_record_id: String,
    pub participants: Vec<ParticipantBrief>,
}

#[derive(Deserialize)]
pub struct FinalizeSharePayload {
    pub category_id: String,
}

#[derive(Serialize)]
pub struct ShareStatusResponse {
    pub participant_id: String,
    pub settled: bool,
    pub finalized: bool,
}

#[derive(Deserialize)]
pub struct PendingSharesQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Deserialize)]
pub struct UnsettledSharesQuery {
    pub friend_id: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Serialize)]
pub struct PendingShare {
    pub participant_id: String,
    pub split_id: String,
    pub description: String,
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub creditor_user_id: String,
    pub creditor_name: String,
    pub settled: bool,
}

#[derive(Serialize)]
pub struct UnsettledShare {
    pub participant_id: String,
    pub split_id: String,
    pub description: String,
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub direction: String,
    pub counterparty_user_id: String,
    pub counterparty_name: String,
    pub finalized: bool,
    pub settled: bool,
}

#[derive(Serialize)]
pub struct PendingShareListResponse {
    pub shares: Vec<PendingShare>,
    pub total_count: u32,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Serialize)]
pub struct UnsettledShareListResponse {
    pub shares: Vec<UnsettledShare>,
    pub total_count: u32,
    pub limit: u32,
    pub offset: u32,
}
