use crate::error::Result;
use crate::llm::{LlmRequest, StructuredOutput};

pub const ANALYZE_PROMPT_ID: &str = "analyze-source-v2";
pub const DRAFT_PROMPT_ID: &str = "generate-drafts-v5";
pub const QUERY_PROMPT_ID: &str = "answer-query-v1";

pub fn analyze_request(source_title: &str, markdown: &str, max_tokens: u64) -> LlmRequest {
    LlmRequest {
        operation: "analyze_source".to_owned(),
        system: ANALYZE_SYSTEM.to_owned(),
        prompt: format!("# Source title\n\n{source_title}\n\n# Source markdown\n\n{markdown}"),
        prompt_template_id: ANALYZE_PROMPT_ID.to_owned(),
        temperature: Some(0.1),
        max_tokens: Some(max_tokens),
        structured_output: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draft_request(
    template_name: &str,
    source_title: &str,
    source_excerpts: &str,
    candidate_json: &str,
    batch_index: usize,
    batch_count: usize,
    related_pages: &[(String, String, String)],
    max_tokens: u64,
) -> Result<LlmRequest> {
    let related = related_pages
        .iter()
        .map(|(path, title, excerpt)| format!("## {path}\n\nTitle: {title}\n\n{excerpt}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(LlmRequest {
        operation: "generate_drafts".to_owned(),
        system: DRAFT_SYSTEM.replace("{template}", template_name),
        prompt: format!(
            "# Source title\n\n{source_title}\n\n# Source excerpts\n\n{source_excerpts}\n\n# Candidates for this batch\n\n{candidate_json}\n\n# Existing related pages\n\n{related}\n\n# Batch\n\n{batch_index} of {batch_count}"
        ),
        prompt_template_id: DRAFT_PROMPT_ID.to_owned(),
        temperature: Some(0.2),
        max_tokens: Some(max_tokens),
        structured_output: Some(draft_response_format()),
    })
}

pub fn query_request(question: &str, context: &[crate::task::QueryContextPage]) -> LlmRequest {
    let evidence = context
        .iter()
        .enumerate()
        .map(|(index, page)| {
            format!(
                "[{}] path: {}\ntitle: {}\npage_type: {}\nrevision: {}\ncontent:\n{}",
                index + 1,
                page.path,
                page.title,
                page.page_type,
                page.revision.key(),
                page.markdown
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    LlmRequest {
        operation: "answer_query".to_owned(),
        system: QUERY_SYSTEM.to_owned(),
        prompt: format!("# Question\n\n{question}\n\n# Evidence\n\n{evidence}"),
        prompt_template_id: QUERY_PROMPT_ID.to_owned(),
        temperature: Some(0.1),
        max_tokens: Some(3000),
        structured_output: None,
    }
}

pub fn draft_response_format() -> StructuredOutput {
    let string_array = serde_json::json!({
        "type": "array",
        "items": {"type": "string"}
    });
    let evidence = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["quote", "section"],
        "properties": {
            "quote": {"type": "string"},
            "section": {"type": "string"}
        }
    });
    let section = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["heading", "paragraphs", "bullets"],
        "properties": {
            "heading": {"type": "string"},
            "paragraphs": {"type": "array", "items": {"type": "string"}},
            "bullets": {"type": "array", "items": {"type": "string"}}
        }
    });
    let page_fields = serde_json::json!({
        "aliases": string_array,
        "tags": string_array,
        "related": string_array,
        "sections": {"type": "array", "items": section},
        "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "importance": {
            "type": "string",
            "enum": ["core", "supporting", "incidental"]
        },
        "evidence": {"type": "array", "items": evidence}
    });
    let mut entity = page_fields.clone();
    let entity_object = entity.as_object_mut().expect("entity fields are an object");
    entity_object.insert("page_type".to_owned(), serde_json::json!("entity"));
    entity_object.insert("name".to_owned(), serde_json::json!({"type": "string"}));
    entity_object.insert("summary".to_owned(), serde_json::json!({"type": "string"}));
    let mut concept = page_fields;
    let concept_object = concept
        .as_object_mut()
        .expect("concept fields are an object");
    concept_object.insert("page_type".to_owned(), serde_json::json!("concept"));
    concept_object.insert("name".to_owned(), serde_json::json!({"type": "string"}));
    concept_object.insert(
        "definition".to_owned(),
        serde_json::json!({"type": "string"}),
    );

    StructuredOutput {
        name: "corpusbot_draft_batch".to_owned(),
        schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["pages"],
            "properties": {
                "pages": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "anyOf": [
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "page_type", "name", "aliases", "summary", "tags",
                                    "related", "sections", "confidence", "importance", "evidence"
                                ],
                                "properties": entity
                            },
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "page_type", "name", "aliases", "definition", "tags",
                                    "related", "sections", "confidence", "importance", "evidence"
                                ],
                                "properties": concept
                            }
                        ]
                    }
                }
            }
        }),
    }
}

const ANALYZE_SYSTEM: &str = r#"You are a precise research analyst. Return only a JSON object, without Markdown fences.
Required shape:
{"title":"string","summary":"string","entities":[{"name":"string","aliases":["string"],"summary":"string","confidence":0.0,"importance":"core|supporting|incidental","evidence":[{"quote":"exact source text","section":"heading"}]}],"concepts":[{"name":"string","aliases":["string"],"definition":"string","confidence":0.0,"importance":"core|supporting|incidental","evidence":[{"quote":"exact source text","section":"heading"}]}]}
Inventory every salient entity and reusable concept. Do not impose an arbitrary page count. Include central, supporting, and incidental candidates when they are explicitly present. Every entity and concept MUST include `aliases`; use `[]` when there are none. Confidence is 0.0-1.0. Evidence quotes must be copied exactly from the source. Use concise evidence-backed wording. Do not invent facts. All JSON string values must be valid JSON. Escape any ASCII double quote inside text as \" or, preferably, use 「」 for quoted terms."#;

const DRAFT_SYSTEM: &str = r#"You are a wiki editor. Return exactly one valid JSON object. Do not emit reasoning, <think>, Markdown fences, or text before or after JSON.
Generate only the candidate pages listed in this batch. Do not add or omit pages.

Use exactly this JSON grammar:
root ::= {"pages": [page]}
page ::= entity_page | concept_page
entity_page ::= {"page_type":"entity","name":"string","aliases":[string],"summary":"string","tags":[string],"related":[string],"sections":[section],"confidence":number,"importance":"core"|"supporting"|"incidental","evidence":[evidence]}
concept_page ::= {"page_type":"concept","name":"string","aliases":[string],"definition":"string","tags":[string],"related":[string],"sections":[section],"confidence":number,"importance":"core"|"supporting"|"incidental","evidence":[evidence]}
section ::= {"heading":"string","paragraphs":[string],"bullets":[string]}
evidence ::= {"quote":"exact source text","section":"string"}

Allowed entity-page keys are exactly: page_type, name, aliases, summary, tags, related, sections, confidence, importance, evidence.
Allowed concept-page keys are exactly: page_type, name, aliases, definition, tags, related, sections, confidence, importance, evidence.
Allowed section keys are exactly: heading, paragraphs, bullets.
`bullets` is allowed only inside a section. A page must not have a page-level `bullets` key. Do not close a page object until every page-level field has been written.
The pages array must contain exactly one object per candidate in this batch.

Core pages need two or three substantive sections. Supporting pages need one or two. Incidental pages may be a concise stub. Keep sections to at most two paragraphs and three bullets. Use only facts stated in the source or candidate metadata. Tags are lowercase kebab-case. Use exact names from Candidate metadata or Existing related pages for related; omit related when uncertain. Do not invent facts. Every returned entity and concept MUST include `aliases`; use `[]` when there are none. All JSON string values must be valid JSON. Escape any ASCII double quote inside text as \" or, preferably, use 「」 for quoted terms."#;

const QUERY_SYSTEM: &str = r#"You are a wiki research assistant. Answer only from numbered evidence.
Return exactly one JSON object and no other content. Do not emit reasoning, <think>, Markdown fences, or text before or after JSON.
{"answer":"string with [number] citations","citations":[{"number":1,"path":"wiki/page.md","quote":"exact evidence quote","revision":{"kind":"content","value":{"sha256":"..."}}}]}
Quotes must be copied exactly from evidence. If evidence is insufficient, use an empty citations array.
Use unescaped double quotes only as JSON string delimiters. Inside any JSON string, escape double quotes as \" or, preferably, use 「」 for quoted terms."#;
