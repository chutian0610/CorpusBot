use crate::error::{AgentError, Result};
use crate::llm::LlmResponse;
use serde::Deserialize;

pub(crate) fn parse_json<T: for<'de> Deserialize<'de>>(response: &LlmResponse) -> Result<T> {
    let mut text = strip_reasoning(&response.text);
    if text.starts_with("```") {
        text = text.trim_start_matches("```json").trim_start_matches("```");
        text = text.trim_end_matches("```").trim();
    }
    let start = text
        .find('{')
        .ok_or_else(|| AgentError::Schema("response does not contain a JSON object".to_owned()))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| AgentError::Schema("response JSON object is unterminated".to_owned()))?;
    if start >= end {
        return Err(AgentError::Schema("response JSON is malformed".to_owned()));
    }
    let json = &text[start..=end];
    match serde_json::from_str::<T>(json) {
        Ok(value) => Ok(value),
        Err(original_error) => {
            let repaired = repair_unescaped_quotes(json);
            let repaired = repair_missing_section_values(repaired);
            let repaired = repair_misnested_page_bullets(repaired);
            serde_json::from_str::<T>(&repaired)
                .map_err(|_| AgentError::Schema(format!("invalid JSON: {original_error}")))
        }
    }
}

pub(crate) fn repair_missing_section_values(json: String) -> String {
    json.replace("{\"paragraph\"}", "{\"paragraphs\":[]}")
        .replace("{\"bullets\"}", "{\"bullets\":[]}")
}

/// Repairs a provider mistake in batch drafts: `sections` is left open, then
/// the page-level fields are emitted as a separate object. Reopening that
/// object as a page-level `bullets` key restores the intended bracket shape.
fn repair_misnested_page_bullets(json: String) -> String {
    json.replace(r#"]},{"bullets":"#, r#"]}],"bullets":"#)
}

/// Repairs the common provider mistake of leaving unescaped ASCII quotes
/// inside string values (for example: `强调"证据"优先`). A quote is only
/// treated as a JSON delimiter when a structural token follows it.
fn repair_unescaped_quotes(json: &str) -> String {
    let mut repaired = String::with_capacity(json.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = json.chars().peekable();

    while let Some(character) = chars.next() {
        if !in_string {
            if character == '"' {
                in_string = true;
            }
            repaired.push(character);
            continue;
        }

        if escaped {
            escaped = false;
            repaired.push(character);
            continue;
        }

        match character {
            '\\' => {
                escaped = true;
                repaired.push(character);
            }
            '"' => {
                let follows_structure = chars
                    .peek()
                    .is_none_or(|next| matches!(next, ',' | ':' | '}' | ']'))
                    || chars
                        .clone()
                        .find(|character| !character.is_whitespace())
                        .is_some_and(|next| matches!(next, ',' | ':' | '}' | ']'));
                if follows_structure {
                    in_string = false;
                    repaired.push(character);
                } else {
                    repaired.push('\\');
                    repaired.push(character);
                }
            }
            character => repaired.push(character),
        }
    }

    repaired
}

fn strip_reasoning(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(start) = trimmed.find("<think>") else {
        return trimmed;
    };
    let Some(end_offset) = trimmed[start..].find("</think>") else {
        return trimmed;
    };
    let end = start + end_offset + "</think>".len();
    trimmed[end..].trim()
}
