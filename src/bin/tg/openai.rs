use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;

use crate::constants::{DEFAULT_WHISPER_MODEL, TOOL_MAX_ROUNDS};
use crate::db::execute_tool_call;
use crate::models::{BotState, CategoryInfo};

#[derive(Deserialize)]
struct WhisperTranscriptionResponse {
    text: String,
}

struct ToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

pub async fn respond_with_tools(
    state: &BotState,
    user_id: &str,
    message: &str,
    image_data_url: Option<&str>,
    categories: &[CategoryInfo],
    history: &[serde_json::Value],
) -> Result<String, String> {
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

    let now_date = OffsetDateTime::now_utc().date().to_string();
    let system_prompt = format!(
        "You are a budget assistant for a Telegram bot.\n\
         You can use three tools: create_record, edit_record, list_records.\n\
         Decide which tool(s) to use based on the user's request.\n\
         Never fabricate success. For add/edit/list requests, you MUST call the relevant tool first, then reply from tool results only.\n\
         Do not ask for confirmation before editing records. Apply edits directly.\n\
         Never ask the user to use confirm/cancel commands.\n\
         For delete requests, clearly state delete is not supported by this assistant.\n\
         Edit intent rule: when user says \"change to ...\" / \"改成...\" without a field name, treat it as renaming the record, so pass the new value in `name` (not category_name).\n\
         Use concise, friendly replies.\n\
         Output format rules:\n\
         - If create_record succeeds, reply in EXACTLY this block format and nothing else:\n\
           [RECORD_ADDED]\n\
           id: <id>\n\
           name: <name>\n\
           amount: <amount>\n\
           category: <category_name>\n\
           date: <YYYY-MM-DD>\n\
         - If edit_record succeeds, reply in EXACTLY this block format and nothing else:\n\
            [RECORD_EDITED]\n\
            id: <id>\n\
            name: <name>\n\
            amount: <amount>\n\
            category: <category_name>\n\
            date: <YYYY-MM-DD>\n\
         - Never output [RECORD_ADDED] or [RECORD_EDITED] unless the corresponding tool returned ok=true.\n\
         - If multiple records are added/edited, repeat the same block for each record with one blank line between blocks.\n\
         - If a tool returns an error, reply in EXACTLY this format and nothing else:\n\
           [ERROR]\n\
           message: <error message>\n\n\
         Timezone: {}\n\
         Current date (YYYY-MM-DD): {}\n\n\
         Categories:\n{}",
        state.timezone, now_date, category_list
    );

    let mut input_messages: Vec<serde_json::Value> = Vec::new();
    input_messages.push(json!({
        "role": "system",
        "content": system_prompt
    }));
    input_messages.extend_from_slice(history);

    let user_content = match image_data_url {
        Some(image_url) => {
            let mut blocks = Vec::new();
            blocks.push(json!({
                "type": "input_image",
                "image_url": image_url,
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

    let tools = build_tools_schema();
    let mut previous_response_id: Option<String> = None;
    let mut input = json!(input_messages);

    for _round in 0..TOOL_MAX_ROUNDS {
        let response_value = send_responses_request(
            &state.http,
            &state.openai_api_key,
            &state.openai_model,
            &state.openai_reasoning_effort,
            input,
            &tools,
            previous_response_id.as_deref(),
        )
        .await?;

        if let Some(response_id) = response_value.get("id").and_then(|value| value.as_str()) {
            previous_response_id = Some(response_id.to_string());
        }

        let tool_calls = extract_tool_calls(&response_value);
        if tool_calls.is_empty() {
            let reply = extract_output_text(&response_value)?;
            if reply.trim().is_empty() {
                return Ok("I couldn't generate a reply. Please try again.".to_string());
            }
            return Ok(reply.trim().to_string());
        }

        let mut tool_outputs = Vec::new();
        for tool_call in tool_calls {
            let tool_output = match execute_tool_call(
                state,
                user_id,
                &tool_call.name,
                &tool_call.arguments,
            )
            .await
            {
                Ok(result) => result,
                Err(message) => json!({
                    "ok": false,
                    "error": message,
                }),
            };

            tool_outputs.push(json!({
                "type": "function_call_output",
                "call_id": tool_call.call_id,
                "output": tool_output.to_string(),
            }));
        }

        input = json!(tool_outputs);
    }

    Err("Tool-call loop limit reached. Please try a simpler request.".to_string())
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

fn build_tools_schema() -> serde_json::Value {
    json!([
        {
            "type": "function",
            "name": "create_record",
            "description": "Create one income/expense record.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "amount": { "type": "number" },
                    "category_id": { "type": "string" },
                    "category_name": { "type": "string" },
                    "date": { "type": "string", "description": "YYYY-MM-DD" },
                    "is_income": { "type": "boolean", "description": "Required only when creating a new category by category_name." }
                },
                "required": ["name", "amount"],
                "additionalProperties": false
            }
        },
        {
            "type": "function",
            "name": "edit_record",
            "description": "Edit an existing record immediately. No confirmation step.",
            "parameters": {
                "type": "object",
                "properties": {
                    "record_id": { "type": "string", "description": "Preferred target identifier for the record to edit." },
                    "record_name": { "type": "string", "description": "Fallback target identifier when record_id is unknown." },
                    "name": { "type": "string" },
                    "amount": { "type": "number" },
                    "category_id": { "type": "string" },
                    "category_name": { "type": "string" },
                    "date": { "type": "string", "description": "YYYY-MM-DD" }
                },
                "additionalProperties": false
            }
        },
        {
            "type": "function",
            "name": "list_records",
            "description": "List records with optional filters.",
            "parameters": {
                "type": "object",
                "properties": {
                    "start_date": { "type": "string", "description": "YYYY-MM-DD" },
                    "end_date": { "type": "string", "description": "YYYY-MM-DD" },
                    "limit": { "type": "integer" },
                    "offset": { "type": "integer" },
                    "category_id": { "type": "string" },
                    "category_name": { "type": "string" },
                    "name_contains": { "type": "string" },
                    "min_amount": { "type": "number" },
                    "max_amount": { "type": "number" }
                },
                "additionalProperties": false
            }
        }
    ])
}

async fn send_responses_request(
    http: &Client,
    api_key: &str,
    model: &str,
    reasoning_effort: &str,
    input: serde_json::Value,
    tools: &serde_json::Value,
    previous_response_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let mut body = json!({
        "model": model,
        "reasoning": {
            "effort": reasoning_effort
        },
        "tool_choice": "auto",
        "tools": tools,
        "input": input
    });

    if let Some(response_id) = previous_response_id
        && let Some(map) = body.as_object_mut()
    {
        map.insert("previous_response_id".to_string(), json!(response_id));
    }

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

    response
        .json()
        .await
        .map_err(|_| "Failed to parse OpenAI response".to_string())
}

fn extract_tool_calls(value: &serde_json::Value) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    let Some(outputs) = value.get("output").and_then(|output| output.as_array()) else {
        return calls;
    };

    for output in outputs {
        if output.get("type").and_then(|value| value.as_str()) != Some("function_call") {
            continue;
        }

        let Some(name) = output.get("name").and_then(|value| value.as_str()) else {
            continue;
        };

        let Some(call_id) = output
            .get("call_id")
            .and_then(|value| value.as_str())
            .or_else(|| output.get("id").and_then(|value| value.as_str()))
        else {
            continue;
        };

        let arguments = match output.get("arguments") {
            Some(argument_value) if argument_value.is_string() => {
                argument_value.as_str().unwrap_or("{}").to_string()
            }
            Some(argument_value) => argument_value.to_string(),
            None => "{}".to_string(),
        };

        calls.push(ToolCall {
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments,
        });
    }

    calls
}

fn extract_output_text(value: &serde_json::Value) -> Result<String, String> {
    if let Some(output_text) = value.get("output_text").and_then(|text| text.as_str())
        && !output_text.trim().is_empty()
    {
        return Ok(output_text.to_string());
    }

    let outputs = value
        .get("output")
        .and_then(|output| output.as_array())
        .ok_or_else(|| "OpenAI response missing output".to_string())?;

    let mut parts = Vec::new();
    for output in outputs {
        if output.get("type").and_then(|value| value.as_str()) != Some("message") {
            continue;
        }

        let Some(content_items) = output.get("content").and_then(|content| content.as_array())
        else {
            continue;
        };

        for content in content_items {
            if let Some(kind) = content.get("type").and_then(|value| value.as_str()) {
                if kind == "output_text" || kind == "text" {
                    if let Some(text) = content.get("text").and_then(|value| value.as_str())
                        && !text.trim().is_empty()
                    {
                        parts.push(text.to_string());
                    }
                }
            }
        }
    }

    if parts.is_empty() {
        Err("OpenAI response missing assistant text".to_string())
    } else {
        Ok(parts.join("\n"))
    }
}
