// main.rs
use std::io::{self, Write};
use std::process::Command;
use std::path::PathBuf;
use tokio::signal::unix::{signal, SignalKind};
use dirs_next as dirs;
use serde_json::{self, Value, json};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use std::collections::HashMap;
use anyhow::Result as AnyhowResult;

// ANSI color codes
pub const LIGHT_BLUE: &str = "\x1b[94m";
pub const YELLOW: &str = "\x1b[33m";
pub const RESET_COLOR: &str = "\x1b[0m";

// Constants
const MODEL_NAME: &str = "Echo";
const API_URL: &str = "http://localhost:8080/v1/chat/completions";

const SYSTEM_PROMPT: &str = r#"
You are Echo, Eric's local sidekick and friend.

Only run one tool per turn.
Never start a session with a tool call and run ONE tool and WAIT for user input.

Always ASK for permission BEFORE executing and tool responses do not count as approval.
We go one step at a time: summarise findings and ask permission before each step.

You have one powerful tool:
- Execute shell commands using: COMMAND: [full command here]

IMPORTANT RULES:
- I am not running the tools — you suggest them.
- Output the tool call in exactly this format and nothing else on that line:
  COMMAND: the exact command you want to run
- Stop after you output a tool call. Wait for the result before deciding the next step.
- For right now, stop and ask before you run each tool.
- After you receive the result, continue reasoning or give your final answer.
- Do NOT hallucinate command output — always use the tool when you need real system info.
- You can run ANY command the user has permission to execute.
- For large outputs, feel free to redirect to files (>, >>) and tell me the filename.
- Do NOT chain actions across turns without approval.

You have 2 Echo memory files to use across sessions:
- ~/Documents/Echo_short_term_memory.txt — for the current job (in case of session failure).
- ~/Documents/Echo_long_term_memory.txt — for permanent knowledge you want to keep across jobs.

You can and should read them using the cat command just like any other tool.

Internet-related tasks: use ddgr, lynx, curl, wget, etc. when needed.

=== NEW: Session Support ===
You can also use persistent sessions with this exact format:
  SESSION:NAME command here

Examples:
SESSION:msf msfconsole -q
SESSION:shell whoami && pwd
SESSION:recon nmap -sV 192.168.1.0/24

Once a session is created, continue using the same SESSION:NAME for follow-up commands in that session.

Prefer COMMAND: for simple one-off commands.
Use SESSION:NAME when you need a persistent or interactive session (like msfconsole).

Always use only ONE tool or session call per response.
"#;

pub static ACTIVE_SESSIONS: Lazy<Mutex<HashMap<String, (String, String)>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static SHUTDOWN_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tokio::main]
async fn main() -> AnyhowResult<()> {
    println!("Echo Rust Wrapper v2 – Async Tool Calls with Named Pipes");
    println!("Type 'quit' or 'exit' to stop.\n");

    // Handle graceful shutdowns
    let mut termination = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");
    let mut interrupt = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = termination.recv() => { SHUTDOWN_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst); break; },
                _ = interrupt.recv() => { SHUTDOWN_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst); break; }
            }
        }
    });

    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/eric/Documents"));
    let context_path = PathBuf::from("/home/eric/echo/Echo_rag/Echo-context.txt");
    let mut context_content = String::new();

    if tokio::fs::metadata(&context_path).await.is_ok() {
        context_content = tokio::fs::read_to_string(&context_path)
            .await
            .expect("Failed to read context file");
        println!("✅ Loaded context file: {}", context_path.display());
    } else {
        println!("⚠️ Context file not found at: {}", context_path.display());
    }

    tokio::fs::create_dir_all(home_dir.join("Documents"))
        .await
        .expect("Failed to create Documents dir");

    let full_system_prompt = format!("{}\n\n{}", SYSTEM_PROMPT.trim(), context_content.trim());

    //save_chat_log_entry(&home_dir, "", &full_system_prompt, "SESSION_START").await?;

    let mut messages = vec![
        json!({"role": "system", "content": full_system_prompt}),
    ];

    println!("Echo: Ready. Type 'quit' or 'exit' to end session.\n");

    loop {
        print!("You: ");
        io::stdout().flush()?;
        let mut user_input = String::new();
        io::stdin()
            .read_line(&mut user_input)
            .expect("Failed to read line");
        let trimmed_input = user_input.trim();

        // Exit check
        if trimmed_input.eq_ignore_ascii_case("quit") || trimmed_input.eq_ignore_ascii_case("exit") {
            println!("Session ended.");
            save_chat_log_entry(&home_dir, "", "", "SESSION_END").await.unwrap();
            break;
        }

        if SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
            println!("\nGraceful shutdown initiated...");
            clean_up_sessions().await?;
            println!("All sessions terminated. Goodbye!");
            return Ok(());
        }

        messages.push(json!({
            "role": "user",
            "content": trimmed_input,
        }));

        let payload = json!({
            "model": MODEL_NAME,
            "messages": &messages,
            "temperature": 0.6,
            "max_tokens": 2048
        });

        let response_text = match reqwest::Client::new()
            .post(API_URL)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(res) => {
                if res.status().is_success() {
                    let body_str = res.text().await.unwrap_or_default();
                    match serde_json::from_str::<Value>(&body_str) {
                        Ok(parsed) => parsed["choices"][0]["message"]["content"]
                            .as_str()
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        Err(_) => "Invalid JSON from API response.".to_string(),
                    }
                } else {
                    format!("API request failed with status: {}", res.status())
                }
            }
            Err(e) => format!(
                "Request to {} failed: {}. Is your local model server running?",
                API_URL, e
            ),
        };

        // === TOOL CALL DETECTION ===
        if let Some((session_name, command)) = extract_session_command(&response_text) {
            println!("{}Echo: Creating/reusing session '{}' and running '{}'.{}", LIGHT_BLUE, &session_name, &command, RESET_COLOR);

            if let Err(e) = is_command_safe(&command) {
                println!("{}Safety block: {}{}", YELLOW, e, RESET_COLOR);
                save_chat_log_entry(&home_dir, trimmed_input, &format!("Blocked: {}", e), "assistant").await.unwrap();
                messages.push(json!({"role": "assistant", "content": format!("Safety block: {}", e)}));
                continue;
            }

            start_or_reuse_session(home_dir.clone(), &session_name, &command).await?;
            let raw_output = execute_in_session(home_dir.clone(), &session_name, command.to_string()).await?;

            // === NEW: Call summarizer here ===
            let summary = summarize_output(&raw_output).await?;   // we'll add this function

            let result_text = format!("Session '{}' summary:\n{}", session_name, summary);

            println!("{}Echo: Session summary:\n{}{}", LIGHT_BLUE, summary, RESET_COLOR);

            save_chat_log_entry(&home_dir, trimmed_input, &result_text, "assistant").await.unwrap();

            // Send clean summary back to model
            messages.push(json!({
                "role": "tool",
                "content": result_text
            }));

        } else if let Some((session_name, sub_command)) = extract_run_command(&response_text) {
            let full_cmd = format!("run {}", sub_command.trim());

            if let Err(e) = is_command_safe(&full_cmd) {
                println!("{}Safety block: {}{}", YELLOW, e, RESET_COLOR);
                save_chat_log_entry(&home_dir, trimmed_input, &format!("Blocked: {}", e), "assistant").await.unwrap();
                messages.push(json!({"role": "assistant", "content": format!("Safety block: {}", e)}));
                continue;
            }

            let output = execute_in_session(home_dir.clone(), &session_name, full_cmd).await?;
            let result_text = format!("Session '{}' run output:\n{}", session_name, output);

            println!("{}Echo: Session output:\n{}{}", LIGHT_BLUE, output, RESET_COLOR);

            save_chat_log_entry(&home_dir, trimmed_input, &result_text, "assistant").await.unwrap();

            messages.push(json!({
                "role": "assistant",
                "content": result_text
            }));

        } else if let Some(session_name) = extract_end_command(&response_text) {
            let _ = end_session(home_dir.clone(), &session_name).await;
            let result_text = format!("Session '{}' has been terminated.", session_name);

            println!("{}Echo: {}", LIGHT_BLUE, result_text);

            save_chat_log_entry(&home_dir, trimmed_input, &result_text, "assistant").await.unwrap();

            messages.push(json!({
                "role": "assistant",
                "content": result_text
            }));

        } else if let Some(command) = extract_command(&response_text) {
            println!("{}Echo: Executing command:{}\n{}\n{}", LIGHT_BLUE, RESET_COLOR, command.trim(), RESET_COLOR);

            // SAFETY CHECK
            if let Err(e) = is_command_safe(&command) {
                println!("{}Safety block: {}{}", YELLOW, e, RESET_COLOR);
                save_chat_log_entry(&home_dir, trimmed_input, &format!("Blocked: {}", e), "assistant").await.unwrap();
                messages.push(json!({"role": "assistant", "content": format!("Safety block: {}", e)}));
                continue;
            }

            let output_cmd = Command::new("sh")
                .arg("-c")
                .arg(command.trim())
                .output()
                .expect("Failed to execute command");

            let stdout = String::from_utf8_lossy(&output_cmd.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output_cmd.stderr).to_string();

            if !stdout.is_empty() {
                println!("{}Echo:\n{}\n{}", LIGHT_BLUE, &stdout.trim(), RESET_COLOR);
            }
            if !stderr.is_empty() {
                println!("{}Errors/Warnings:\n{}\n---", YELLOW, &stderr.trim());
            }

            let full_response = format!("[COMMAND_OUTPUT]\nSTDOUT:\n{}\nSTDERR:\n{}", stdout, stderr);

            save_chat_log_entry(&home_dir, trimmed_input, &full_response, "assistant").await.unwrap();

            messages.push(json!({
                "role": "assistant",
                "content": full_response
            }));

        } else {
            // Plain text response
            println!("{}Echo:\n{}\n{}", LIGHT_BLUE, response_text.trim(), RESET_COLOR);

            save_chat_log_entry(&home_dir, trimmed_input, &response_text, "assistant").await.unwrap();

            messages.push(json!({
                "role": "assistant",
                "content": &response_text,
            }));
        }
    }

    clean_up_sessions().await?;
    println!("\nSession ended normally. Goodbye!");

    Ok(())

}

// Real summarizer - calls the small model on port 8082
async fn summarize_output(raw_output: &str) -> AnyhowResult<String> {
    let payload = json!({
        "model": "summarizer",   // change to your actual model name if different
        "messages": [
            {
                "role": "system",
                "content": "Create a high signal summary of the fact of the output."
            },
            {
                "role": "user",
                "content": raw_output
            }
        ],
        "temperature": 0.3,
        "max_tokens": 1200

    });

    let response = match reqwest::Client::new()
        .post("http://localhost:8082/v1/chat/completions")
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
    {
        Ok(res) => {
            if res.status().is_success() {
                let body = res.text().await.unwrap_or_default();
                match serde_json::from_str::<Value>(&body) {
                    Ok(parsed) => parsed["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("Summary failed.")
                        .trim()
                        .to_string(),
                    Err(_) => "Failed to parse summarizer response.".to_string(),
                }
            } else {
                format!("Summarizer returned status: {}", res.status())
            }
        }
        Err(e) => format!("Failed to connect to summarizer on port 8082: {}", e),
    };

    Ok(response)
}



mod sessions;
mod log;
mod commands;
mod safety;

use sessions::{start_or_reuse_session, execute_in_session, end_session, clean_up_sessions};
use log::save_chat_log_entry;
use commands::{extract_session_command, extract_run_command, extract_end_command, extract_command};
use safety::is_command_safe;
