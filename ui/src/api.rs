use urlencoding::encode;
use wasm_bindgen::JsCast;

use crate::models::*;

const API_BASE: &str = "http://localhost:3000";

fn build_query_params(params: &[(&str, Option<String>)]) -> String {
    let query_parts: Vec<String> = params
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|v| format!("{}={}", encode(key), encode(v))))
        .collect();

    if query_parts.is_empty() {
        String::new()
    } else {
        format!("?{}", query_parts.join("&"))
    }
}

// Helper to make requests with credentials
async fn request<T: serde::de::DeserializeOwned>(
    method: &str,
    url: &str,
    body: Option<String>,
) -> Result<T, String> {
    use web_sys::{RequestCredentials, RequestInit, RequestMode};

    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_mode(RequestMode::Cors);
    opts.set_credentials(RequestCredentials::Include);

    if let Some(b) = body {
        let body_js = wasm_bindgen::JsValue::from_str(&b);
        opts.set_body(&body_js);
        let headers = web_sys::Headers::new().map_err(|_| "Failed to create headers")?;
        headers
            .set("Content-Type", "application/json")
            .map_err(|_| "Failed to set header")?;
        opts.set_headers(&headers);
    }

    let window = web_sys::window().ok_or("No window")?;
    let request =
        web_sys::Request::new_with_str_and_init(url, &opts).map_err(|_| "Failed to create request")?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Fetch failed")?;

    let resp: web_sys::Response = resp_value.dyn_into().map_err(|_| "Invalid response")?;

    if !resp.ok() {
        let text = wasm_bindgen_futures::JsFuture::from(
            resp.text().map_err(|_| "Failed to get text")?,
        )
        .await
        .map_err(|_| "Failed to read text")?;
        let error_text = text.as_string().unwrap_or_else(|| "Request failed".to_string());
        return Err(error_text);
    }

    let json = wasm_bindgen_futures::JsFuture::from(resp.json().map_err(|_| "Failed to get json")?)
        .await
        .map_err(|_| "Failed to parse json")?;

    serde_wasm_bindgen::from_value(json).map_err(|e| e.to_string())
}

async fn request_no_body(method: &str, url: &str) -> Result<(), String> {
    use web_sys::{RequestCredentials, RequestInit, RequestMode};

    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_mode(RequestMode::Cors);
    opts.set_credentials(RequestCredentials::Include);

    let window = web_sys::window().ok_or("No window")?;
    let request =
        web_sys::Request::new_with_str_and_init(url, &opts).map_err(|_| "Failed to create request")?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Fetch failed")?;

    let resp: web_sys::Response = resp_value.dyn_into().map_err(|_| "Invalid response")?;

    if !resp.ok() {
        let text = wasm_bindgen_futures::JsFuture::from(
            resp.text().map_err(|_| "Failed to get text")?,
        )
        .await
        .map_err(|_| "Failed to read text")?;
        let error_text = text.as_string().unwrap_or_else(|| "Request failed".to_string());
        return Err(error_text);
    }

    Ok(())
}

// Auth API

pub async fn login(payload: LoginPayload) -> Result<PublicUser, String> {
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    request("POST", &format!("{}/auth/login", API_BASE), Some(body)).await
}

pub async fn register(payload: RegisterPayload) -> Result<PublicUser, String> {
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    request("POST", &format!("{}/auth/register", API_BASE), Some(body)).await
}

pub async fn get_me() -> Result<PublicUser, String> {
    request("GET", &format!("{}/auth/me", API_BASE), None).await
}

pub async fn logout() -> Result<(), String> {
    request_no_body("POST", &format!("{}/auth/logout", API_BASE)).await
}

// Records API

pub async fn get_records(
    start_date: Option<String>,
    end_date: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<GetRecordsResponse, String> {
    let params = [
        ("start_date", start_date),
        ("end_date", end_date),
        ("limit", limit.map(|v| v.to_string())),
        ("offset", offset.map(|v| v.to_string())),
    ];
    let query = build_query_params(&params);
    let url = format!("{}/records{}", API_BASE, query);
    request("GET", &url, None).await
}

pub async fn create_record(payload: CreateRecordPayload) -> Result<Record, String> {
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    request("POST", &format!("{}/records", API_BASE), Some(body)).await
}

pub async fn update_record(id: &str, payload: UpdateRecordPayload) -> Result<Record, String> {
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    request("PUT", &format!("{}/records/{}", API_BASE, id), Some(body)).await
}

pub async fn delete_record(id: &str) -> Result<(), String> {
    request_no_body("DELETE", &format!("{}/records/{}", API_BASE, id)).await
}

// Categories API

pub async fn get_categories(
    limit: Option<u32>,
    offset: Option<u32>,
    search: Option<&str>,
) -> Result<GetCategoriesResponse, String> {
    let params = [
        ("limit", limit.map(|v| v.to_string())),
        ("offset", offset.map(|v| v.to_string())),
        ("search", search.map(|v| v.to_string())),
    ];
    let query = build_query_params(&params);
    let url = format!("{}/categories{}", API_BASE, query);
    request("GET", &url, None).await
}

pub async fn create_category(payload: CreateCategoryPayload) -> Result<Category, String> {
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    request("POST", &format!("{}/categories", API_BASE), Some(body)).await
}

pub async fn update_category(id: &str, payload: UpdateCategoryPayload) -> Result<Category, String> {
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    request("PUT", &format!("{}/categories/{}", API_BASE, id), Some(body)).await
}

pub async fn delete_category(id: &str) -> Result<(), String> {
    request_no_body("DELETE", &format!("{}/categories/{}", API_BASE, id)).await
}
