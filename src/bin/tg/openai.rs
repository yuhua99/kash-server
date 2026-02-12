use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;

use crate::constants::{DEFAULT_WHISPER_MODEL, SIMILAR_RECORDS_DAYS};
use crate::models::{
    AiBatchRecordResult, AiCategoryHint, AiEditResult, BotState, CategoryInfo, SimilarRecord,
};

use my_budget_server::models::Record;

#[derive(Deserialize)]
struct WhisperTranscriptionResponse {
    text: String,
}

// ---------------------------------------------------------------------------
// Classify message (quick category + amount hint)
// ---------------------------------------------------------------------------

pub async fn classify_message(
    http: &Client,
    api_key: &str,
    model: &str,
    reasoning_effort: &str,
    timezone: &str,
    message: &str,
    categories: &[CategoryInfo],
) -> Result<AiCategoryHint, String> {
    let category_list = categories
        .iter()
        .map(|c| format!("- {} | {} | is_income={}", c.id, c.name, c.is_income))
        .collect::<Vec<_>>()
        .join("\n");

    let input = format!(
        "You are a finance assistant. Extract amount and category for the user's message.\n\n\
         User message:\n{}\n\n\
         Categories:\n{}\n\n\
         Timezone: {}\n\n\
         Rules:\n\
         - Choose the best category_id from the list.\n\
         - If unsure about category, set category_id and category_name to empty strings.\n\
         - If amount is missing, set amount to 0.\n",
        message, category_list, timezone
    );

    let schema = json!({
        "type": "object",
        "properties": {
            "amount": { "type": "number" },
            "category_id": { "type": "string" },
            "category_name": { "type": "string" },
            "is_income": { "type": "boolean" }
        },
        "required": ["amount", "category_id", "category_name", "is_income"],
        "additionalProperties": false
    });

    call_openai_json(
        http,
        api_key,
        model,
        reasoning_effort,
        "category_hint",
        json!(input),
        schema,
    )
    .await
}

// ---------------------------------------------------------------------------
// Extract records from message
// ---------------------------------------------------------------------------

pub async fn extract_records(
    state: &BotState,
    message: &str,
    image_data_url: Option<&str>,
    categories: &[CategoryInfo],
    similar_records: &[SimilarRecord],
    hint: Option<&AiCategoryHint>,
    history: &[serde_json::Value],
) -> Result<AiBatchRecordResult, String> {
    let category_list = if categories.is_empty() {
        "(none)".to_string()
    } else {
        categories
            .iter()
            .map(|c| format!("- {} | {} | is_income={}", c.id, c.name, c.is_income))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let similar_list = if similar_records.is_empty() {
        "(none)".to_string()
    } else {
        similar_records
            .iter()
            .map(|r| format!("- {} ({})", r.name, r.amount))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let now_date = OffsetDateTime::now_utc().date().to_string();
    let hint_text = match hint {
        Some(hint) => format!(
            "Pre-extracted: amount={}, category_id='{}', category_name='{}', is_income={}\n",
            hint.amount, hint.category_id, hint.category_name, hint.is_income
        ),
        None => "Pre-extracted: (none)\n".to_string(),
    };

    let system_prompt = format!(
        "You are a finance assistant. Convert the user's message into one or more expense/income records.\n\n\
        Categories:\n{category_list}\n\n\
        Similar records (same category, similar amount, last {similar_days} days):\n{similar_list}\n\n\
        {hint_text}\
        Timezone: {timezone}\n\
        Current date (YYYY-MM-DD): {now_date}\n\n\
        Rules:\n\
        - Extract every record the user mentions into the \"records\" array. If the user lists multiple items with amounts, include each one as a separate record.\n\
        - The user may attach a receipt/invoice photo. If an image is provided, read it and extract all valid records you can infer from it.\n\
        - Return each record's date in YYYY-MM-DD format.\n\
        - If a similar record name matches, reuse its exact name.\n\
        - Choose the best matching category_id from the categories list. If no category fits, set needs_clarification=true and ask the user which category to use.\n\
        - Use conversation history to resolve references to previous messages.\n\
        - Set needs_clarification=true only when essential information is missing (e.g. no amount, or category cannot be determined). Do not use it merely to confirm what the user already stated clearly.\n",
        category_list = category_list,
        similar_days = SIMILAR_RECORDS_DAYS,
        similar_list = similar_list,
        hint_text = hint_text,
        timezone = state.timezone,
        now_date = now_date,
    );

    let mut input_messages: Vec<serde_json::Value> = Vec::new();
    input_messages.push(json!({
        "role": "system",
        "content": system_prompt
    }));
    input_messages.extend_from_slice(history);
    let user_content = match image_data_url {
        Some(image_data_url) => {
            let mut blocks = Vec::new();
            blocks.push(json!({
                "type": "input_image",
                "image_url": image_data_url,
            }));
            if !message.trim().is_empty() {
                blocks.push(json!({
                    "type": "input_text",
                    "text": message,
                }));
            }
            json!(blocks)
        }
        None => json!(message),
    };

    input_messages.push(json!({
        "role": "user",
        "content": user_content
    }));

    let schema = json!({
        "type": "object",
        "properties": {
            "records": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "amount": { "type": "number" },
                        "category_id": { "type": "string" },
                        "category_name": { "type": "string" },
                        "date": { "type": "string" },
                        "is_income": { "type": "boolean" }
                    },
                    "required": ["name", "amount", "category_id", "category_name", "date", "is_income"],
                    "additionalProperties": false
                }
            },
            "needs_clarification": { "type": "boolean" },
            "clarification": { "type": "string" }
        },
        "required": ["records", "needs_clarification", "clarification"],
        "additionalProperties": false
    });

    call_openai_json(
        &state.http,
        &state.openai_api_key,
        &state.openai_model,
        &state.openai_reasoning_effort,
        "records",
        json!(input_messages),
        schema,
    )
    .await
}

pub async fn transcribe_voice(
    http: &Client,
    api_key: &str,
    audio_bytes: Vec<u8>,
    file_name: &str,
) -> Result<String, String> {
    let file_part = reqwest::multipart::Part::bytes(audio_bytes)
        .file_name(file_name.to_string())
        .mime_str("audio/ogg")
        .map_err(|_| "Failed to prepare audio upload".to_string())?;

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", DEFAULT_WHISPER_MODEL.to_string())
        .text("response_format", "json".to_string());

    let response = http
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|_| "Failed to contact OpenAI transcription API".to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI transcription error: {} {}", status, text));
    }

    let payload: WhisperTranscriptionResponse = response
        .json()
        .await
        .map_err(|_| "Failed to parse transcription response".to_string())?;

    Ok(payload.text.trim().to_string())
}

// ---------------------------------------------------------------------------
// Extract edit from message
// ---------------------------------------------------------------------------

pub async fn extract_edit(
    state: &BotState,
    message: &str,
    categories: &[CategoryInfo],
    records: &[Record],
    history: &[serde_json::Value],
) -> Result<AiEditResult, String> {
    let category_list = if categories.is_empty() {
        "(none)".to_string()
    } else {
        categories
            .iter()
            .map(|category| {
                format!(
                    "- {} | {} | is_income={}",
                    category.id, category.name, category.is_income
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let record_list = if records.is_empty() {
        "(none)".to_string()
    } else {
        records
            .iter()
            .map(|record| {
                format!(
                    "- {} | {} | amount={} | category_id={} | date={}",
                    record.id, record.name, record.amount, record.category_id, record.date
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let system_prompt = format!(
        "You are a finance assistant. Extract one edit operation from the user's message.\n\n\
         Categories:\n{}\n\n\
         Recent records:\n{}\n\n\
         Rules:\n\
         - target_type must be one of: record, category, none.\n\
         - Only one target is allowed. If multiple edits are requested, set needs_clarification=true.\n\
         - Never perform delete operations. If user asks delete, set needs_clarification=true.\n\
         - For category edits, only name change is allowed.\n\
         - For record edits, any of name/amount/category/date may be changed.\n\
         - If unknown target, set needs_clarification=true with a short clarification message.\n\
         - The user may refer to previous messages in the conversation. Use conversation history to resolve references.\n",
        category_list, record_list
    );

    let mut input_messages: Vec<serde_json::Value> = Vec::new();
    input_messages.push(json!({
        "role": "system",
        "content": system_prompt
    }));
    input_messages.extend_from_slice(history);
    input_messages.push(json!({
        "role": "user",
        "content": message
    }));

    let schema = json!({
        "type": "object",
        "properties": {
            "target_type": { "type": "string" },
            "target_id": { "type": "string" },
            "target_name": { "type": "string" },
            "category_id": { "type": "string" },
            "category_name": { "type": "string" },
            "new_name": { "type": ["string", "null"] },
            "new_amount": { "type": ["number", "null"] },
            "new_category_id": { "type": ["string", "null"] },
            "new_category_name": { "type": ["string", "null"] },
            "new_date": { "type": ["string", "null"] },
            "needs_clarification": { "type": "boolean" },
            "clarification": { "type": "string" }
        },
        "required": [
            "target_type",
            "target_id",
            "target_name",
            "category_id",
            "category_name",
            "new_name",
            "new_amount",
            "new_category_id",
            "new_category_name",
            "new_date",
            "needs_clarification",
            "clarification"
        ],
        "additionalProperties": false
    });

    call_openai_json(
        &state.http,
        &state.openai_api_key,
        &state.openai_model,
        &state.openai_reasoning_effort,
        "edit",
        json!(input_messages),
        schema,
    )
    .await
}

// ---------------------------------------------------------------------------
// OpenAI API call (generic JSON schema)
// ---------------------------------------------------------------------------

pub async fn call_openai_json<T: for<'de> Deserialize<'de>>(
    http: &Client,
    api_key: &str,
    model: &str,
    reasoning_effort: &str,
    schema_name: &str,
    input: serde_json::Value,
    schema: serde_json::Value,
) -> Result<T, String> {
    let body = json!({
        "model": model,
        "reasoning": {
            "effort": reasoning_effort
        },
        "input": input,
        "text": {
            "format": {
                "type": "json_schema",
                "name": schema_name,
                "strict": true,
                "schema": schema
            }
        }
    });

    let response = http
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|_| "Failed to contact OpenAI".to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI error: {} {}", status, text));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "Failed to parse OpenAI response".to_string())?;

    let payload = extract_output_json(&value)?;
    serde_json::from_value(payload).map_err(|_| "Failed to parse OpenAI output".to_string())
}

fn extract_output_json(value: &serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(output_text) = value.get("output_text").and_then(|v| v.as_str()) {
        return serde_json::from_str(output_text)
            .map_err(|_| "Failed to parse OpenAI output".to_string());
    }

    let outputs = value
        .get("output")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "OpenAI response missing output".to_string())?;

    for output in outputs {
        if let Some(contents) = output.get("content").and_then(|v| v.as_array()) {
            for content in contents {
                if let Some(kind) = content.get("type").and_then(|v| v.as_str()) {
                    if kind == "output_json"
                        && let Some(json_value) = content.get("json")
                    {
                        return Ok(json_value.clone());
                    }
                    if kind == "output_text"
                        && let Some(text_value) = content.get("text").and_then(|v| v.as_str())
                    {
                        return serde_json::from_str(text_value)
                            .map_err(|_| "Failed to parse OpenAI output".to_string());
                    }
                }
            }
        }
    }

    Err("OpenAI response missing content".to_string())
}
