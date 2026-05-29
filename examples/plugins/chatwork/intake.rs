use anyhow::{Result, anyhow};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde_json::{Value, json};

use crate::backend::{Annotation, NewTaskInput};
use crate::plugin::{PluginExtra, PluginId};

const CHATWORK_PLUGIN_ID: PluginId = "chatwork";

#[derive(Debug, Clone)]
pub struct ChatworkMessage {
    pub room_id: u64,
    pub message_id: String,
    pub source_url: String,
    pub sender_account_id: u64,
    pub sender_name: String,
    pub body: String,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskDraft {
    pub input: NewTaskInput,
    pub extra: PluginExtra,
    #[allow(dead_code)]
    pub annotations: Vec<Annotation>,
}

pub trait TaskDraftPlugin {
    fn plugin_id(&self) -> PluginId;
    fn apply(&self, message: &ChatworkMessage, draft: &mut TaskDraft) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChatworkSourcePlugin;

impl TaskDraftPlugin for ChatworkSourcePlugin {
    fn plugin_id(&self) -> PluginId {
        CHATWORK_PLUGIN_ID
    }

    fn apply(&self, message: &ChatworkMessage, draft: &mut TaskDraft) -> Result<()> {
        draft.extra.insert(
            self.plugin_id(),
            "source",
            json!({
                "kind": "chatwork",
                "room_id": message.room_id,
                "message_id": message.message_id,
                "url": message.source_url,
                "sender": {
                    "account_id": message.sender_account_id,
                    "name": message.sender_name,
                },
                "sent_at": message.sent_at.to_rfc3339(),
                "body_raw": message.body,
            }),
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CompanyRequestTemplatePlugin;

impl TaskDraftPlugin for CompanyRequestTemplatePlugin {
    fn plugin_id(&self) -> PluginId {
        CHATWORK_PLUGIN_ID
    }

    fn apply(&self, message: &ChatworkMessage, draft: &mut TaskDraft) -> Result<()> {
        let sections = parse_sections(&message.body);

        if draft.input.title.is_empty()
            && let Some(title) = extract_request_title(&message.body)
        {
            draft.input.title = title;
        }

        if let Some(lines) = sections.get("納期（日付を記載する）") {
            let joined = lines.join("\n");
            let (target_date, deadline) = parse_target_and_deadline(&joined, message.sent_at)?;
            draft.input.target_date = target_date;
            draft.input.deadline = deadline;
        }

        if let Some(lines) = sections.get("改修概要") {
            draft.extra.insert(
                self.plugin_id(),
                "summary",
                Value::String(lines.join("\n").trim().to_string()),
            );
        }

        if let Some(lines) = sections.get("依頼主")
            && let Some(requester) = first_nonempty_line(lines)
        {
            draft.extra.insert(
                self.plugin_id(),
                "requester",
                Value::String(requester.to_string()),
            );
        }

        if let Some(lines) = sections.get("過去の依頼(類似依頼)チャット")
            && let Some(url) = first_nonempty_line(lines)
        {
            draft.extra.insert(
                self.plugin_id(),
                "related_request_url",
                Value::String(url.to_string()),
            );
        }

        if let Some(lines) = sections.get("対象サイト") {
            draft.extra.insert(
                self.plugin_id(),
                "target_sites",
                Value::Array(parse_target_sites(lines)),
            );
        }

        if let Some(lines) = sections.get("改修内容") {
            draft.extra.insert(
                self.plugin_id(),
                "description",
                Value::String(lines.join("\n").trim().to_string()),
            );
        }

        if let Some(lines) = sections.get("改修目的") {
            draft.extra.insert(
                self.plugin_id(),
                "abstract",
                Value::String(lines.join("\n").trim().to_string()),
            );
        }

        if let Some(lines) = sections.get("本番反映") {
            draft.extra.insert(
                self.plugin_id(),
                "production_rollout",
                Value::String(lines.join("\n").trim().to_string()),
            );
        }

        draft.extra.insert(
            self.plugin_id(),
            "template_kind",
            Value::String("company_request".into()),
        );

        Ok(())
    }
}

pub fn build_draft_from_chatwork(message: &ChatworkMessage) -> Result<TaskDraft> {
    let mut draft = TaskDraft::default();
    ChatworkSourcePlugin.apply(message, &mut draft)?;
    CompanyRequestTemplatePlugin.apply(message, &mut draft)?;
    Ok(draft)
}

fn extract_request_title(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|line| line.starts_with("#/"))
        .map(|line| line.trim_start_matches('#').trim().to_string())
}

fn parse_sections(body: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut sections = std::collections::BTreeMap::new();
    let mut current: Option<String> = None;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || matches!(line, "[info]" | "[/info]" | "[code]" | "[/code]") {
            continue;
        }

        if let Some(name) = line.strip_prefix('■') {
            current = Some(name.to_string());
            sections.entry(name.to_string()).or_insert_with(Vec::new);
            continue;
        }

        if let Some(section) = current.as_ref() {
            sections
                .entry(section.clone())
                .or_insert_with(Vec::new)
                .push(line.to_string());
        }
    }

    sections
}

fn parse_target_and_deadline(
    text: &str,
    sent_at: DateTime<Utc>,
) -> Result<(Option<NaiveDate>, Option<NaiveDate>)> {
    let target = extract_jp_month_day_after(text, "希望：", sent_at)?;
    let deadline = extract_jp_month_day_after(text, "マスト：", sent_at)?;
    Ok((target, deadline))
}

fn extract_jp_month_day_after(
    text: &str,
    marker: &str,
    sent_at: DateTime<Utc>,
) -> Result<Option<NaiveDate>> {
    let Some(start) = text.find(marker) else {
        return Ok(None);
    };

    let tail = &text[start + marker.len()..];
    let month_idx = match tail.find('月') {
        Some(index) => index,
        None => return Ok(None),
    };
    let day_idx = match tail.find('日') {
        Some(index) => index,
        None => return Ok(None),
    };

    let month = tail[..month_idx]
        .trim()
        .parse::<u32>()
        .map_err(|err| anyhow!("invalid month near `{marker}`: {err}"))?;
    let day = tail[month_idx + "月".len()..day_idx]
        .trim()
        .parse::<u32>()
        .map_err(|err| anyhow!("invalid day near `{marker}`: {err}"))?;

    Ok(NaiveDate::from_ymd_opt(sent_at.year(), month, day))
}

fn first_nonempty_line(lines: &[String]) -> Option<&str> {
    lines
        .iter()
        .map(String::as_str)
        .find(|line| !line.is_empty())
}

fn parse_target_sites(lines: &[String]) -> Vec<Value> {
    lines
        .iter()
        .filter_map(|line| {
            let raw = line.trim();
            if raw.is_empty() {
                return None;
            }

            let site_code = raw
                .rfind('<')
                .zip(raw.rfind('>'))
                .and_then(|(start, end)| (start < end).then(|| raw[start + 1..end].trim()));

            let before_code = raw
                .rfind('<')
                .map(|index| raw[..index].trim_end())
                .unwrap_or(raw);
            let label = before_code
                .split(':')
                .next()
                .map(str::trim)
                .unwrap_or(before_code);

            Some(json!({
                "label": label,
                "site_code": site_code,
                "raw": raw,
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::Value;

    use super::{ChatworkMessage, build_draft_from_chatwork};

    const SAMPLE_BODY: &str = r#"[To:6023559] 石井将輝さん
#/batch/update_master.bash 正常に動作するようにする
#改修依頼
下記の改修をお願いできますでしょうか？
よろしくお願いします！

[info]
■納期（日付を記載する）
・希望：6月2日 マスト：6月5日
→希望は「ここまでだとありがたい」という期日で、マストは必須期日

■改修概要
・/batch/update_master.bash 正常に動作するようにする

■依頼主
佐藤

■過去の依頼(類似依頼)チャット
https://www.chatwork.com/#!rid36219958-1737709065104039936

■対象サイト
[info]
相鉄不動産販売さん       :219.94.162.164  (mansion9.sakura.ne.jp)<c-sotetsu>
リニュアル仲介さん       :59.106.27.193   (mansion37.sakura.ne.jp)<c-rchukai>
明和リアルエステートさん :153.126.170.17  (mansion64.mansion-library.com)<c-meiwa>
永大ハウス工業さん       :160.16.213.15   (mansion65.mansion-library.com)<c-eidaihouse>
さくらハウジングさん     :160.16.209.140  (mansion69.mansion-library.com)<c-sakura>
中央ベストホームさん     :153.126.130.48  (mansion76.mansion-library.com)<c-chuo>
エステート白馬さん       :160.16.196.209  (mansion77.mansion-library.com)<c-hakuba>
明和地所LPさん           :133.167.108.227 (mansion108.mansion-library.com)<c-meiwa-lp>
[/info]

■改修内容
上記サイトにて以下のようなエラーが出るようになったようです。

■改修目的
・/batch/update_master.bash 正常に動作するようにする

■本番反映
・即本番反映OK
[/info]"#;

    #[test]
    fn builds_task_draft_from_chatwork_message() {
        let message = ChatworkMessage {
            room_id: 36219958,
            message_id: "2111786210627420160".into(),
            source_url: "https://www.chatwork.com/#!rid36219958-2111786210627420160".into(),
            sender_account_id: 1343849,
            sender_name: "佐藤 幸二".into(),
            body: SAMPLE_BODY.into(),
            sent_at: Utc.with_ymd_and_hms(2026, 5, 28, 0, 0, 0).unwrap(),
        };

        let draft = build_draft_from_chatwork(&message).expect("draft");

        assert_eq!(
            draft.input.title,
            "/batch/update_master.bash 正常に動作するようにする"
        );
        assert_eq!(
            draft.input.target_date,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 2)
        );
        assert_eq!(
            draft.input.deadline,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 5)
        );
        assert_eq!(
            draft.extra.get("chatwork", "requester"),
            Some(&Value::String("佐藤".into()))
        );
        assert_eq!(
            draft.extra.get("chatwork", "template_kind"),
            Some(&Value::String("company_request".into()))
        );
        assert_eq!(
            draft
                .extra
                .get("chatwork", "target_sites")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(8)
        );
        assert_eq!(
            draft
                .extra
                .get("chatwork", "source")
                .and_then(Value::as_object)
                .and_then(|source| source.get("kind"))
                .and_then(Value::as_str),
            Some("chatwork")
        );
    }
}
