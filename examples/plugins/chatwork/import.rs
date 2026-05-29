use std::process::Command;

use anyhow::{Context, Result, anyhow};
use chrono::{TimeZone, Utc};
use serde::Deserialize;

use crate::backend::{Task, TaskBackend};

use super::intake::{ChatworkMessage, TaskDraft, build_draft_from_chatwork};

pub fn import_chatwork_url<B: TaskBackend>(backend: &B, url: &str) -> Result<Task> {
    let message = fetch_chatwork_message(url)?;
    let draft = build_draft_from_chatwork(&message)?;
    persist_task_draft(backend, draft)
}

fn fetch_chatwork_message(url: &str) -> Result<ChatworkMessage> {
    let output = Command::new("cw")
        .args(["g", "--format=json-minify", url])
        .output()
        .with_context(|| format!("failed to run `cw g` for {url}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(anyhow!("failed to fetch Chatwork message: {detail}"));
    }

    let response: ChatworkMessageResponse =
        serde_json::from_slice(&output.stdout).context("failed to parse Chatwork message JSON")?;
    response.try_into_message(url)
}

fn persist_task_draft<B: TaskBackend>(backend: &B, draft: TaskDraft) -> Result<Task> {
    let mut task = backend.add(draft.input)?;

    for (key, value) in draft.extra.into_map() {
        task = backend.set_extra(task.id.expect("task id"), &key, value)?;
    }

    Ok(task)
}

#[derive(Debug, Deserialize)]
struct ChatworkMessageResponse {
    account: ChatworkAccount,
    body: String,
    message_id: String,
    send_time: i64,
}

#[derive(Debug, Deserialize)]
struct ChatworkAccount {
    account_id: u64,
    name: String,
}

impl ChatworkMessageResponse {
    fn try_into_message(self, source_url: &str) -> Result<ChatworkMessage> {
        let room_id = parse_room_id_from_url(source_url)?;
        let sent_at = Utc
            .timestamp_opt(self.send_time, 0)
            .single()
            .ok_or_else(|| anyhow!("invalid Chatwork send_time: {}", self.send_time))?;

        Ok(ChatworkMessage {
            room_id,
            message_id: self.message_id,
            source_url: source_url.to_string(),
            sender_account_id: self.account.account_id,
            sender_name: self.account.name,
            body: self.body,
            sent_at,
        })
    }
}

fn parse_room_id_from_url(url: &str) -> Result<u64> {
    let marker = "#!rid";
    let start = url
        .find(marker)
        .ok_or_else(|| anyhow!("unsupported Chatwork URL: {url}"))?
        + marker.len();

    let tail = &url[start..];
    let digits: String = tail.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err(anyhow!("could not parse room id from Chatwork URL: {url}"));
    }

    digits
        .parse::<u64>()
        .map_err(|err| anyhow!("invalid room id in Chatwork URL: {err}"))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{ChatworkMessageResponse, parse_room_id_from_url};

    #[test]
    fn parses_room_id_from_chatwork_url() -> Result<()> {
        let room_id =
            parse_room_id_from_url("https://www.chatwork.com/#!rid36219958-2111786210627420160")?;
        assert_eq!(room_id, 36219958);
        Ok(())
    }

    #[test]
    fn converts_chatwork_response_into_message() -> Result<()> {
        let response = ChatworkMessageResponse {
            account: super::ChatworkAccount {
                account_id: 1343849,
                name: "佐藤 幸二".into(),
            },
            body: "#/batch/update_master.bash 正常に動作するようにする".into(),
            message_id: "2111786210627420160".into(),
            send_time: 1779962667,
        };

        let message = response
            .try_into_message("https://www.chatwork.com/#!rid36219958-2111786210627420160")?;

        assert_eq!(message.room_id, 36219958);
        assert_eq!(message.sender_account_id, 1343849);
        assert_eq!(message.sender_name, "佐藤 幸二");
        Ok(())
    }
}
