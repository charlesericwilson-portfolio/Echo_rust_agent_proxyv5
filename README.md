# Echo_rust_agent_proxy
Continuation of [Echo tmux agentv3](https://github.com/charlesericwilson-portfolio/Echo_tmux_agentv3) and adds poxy tool calls + output summarization. 
```mermaid
flowchart TD
    A[User sends prompt] --> B[LLM / Echo]
    B --> C[LLM generates reply]
    C --> D[Tool Extractor checks for session:NAME or COMMAND:]
    
    D -->|Session command found| E[Session Manager]
    E --> F[Auto-create tmux session if needed]
    F --> G[Send command to tmux session]
    G --> H[Session Manager starts polling tmux pane]
    H --> I[Wait for new output + markers]
    I --> J[Capture only new output between markers]
    J --> K[Update Database with clean output]
    K --> L[Send tool result back to LLM as 'tool' message]
    
    D -->|No session command| M[Execute as normal COMMAND:]
    M --> N[Return output to LLM]
    
    L --> B
    N --> B
    
    style A fill:#4ade80,stroke:#166534
    style B fill:#60a5fa,stroke:#1e40af
    style E fill:#facc15,stroke:#854d0e
    style K fill:#c084fc,stroke:#6b21a8
```
## Echo Rust Wrapper v5 (In Testing)

**Current Version:** Rust v5 (Python proxy was v4)

This is the active development version of **Echo** — a lightweight, local red-team LLM agent wrapper written in Rust.

### What it does
- Supports **hybrid raw-text tool calling**:
  - `COMMAND: <command>` for simple one-shot shell commands
  - `SESSION:NAME <command>` for persistent tmux sessions (ideal for msfconsole, long-running shells, etc.)
- Automatic tmux session creation/reuse
- Marker-based clean output capture (only returns new command output, not full session history)
- Safety deny list (blocks dangerous commands before execution)
- JSONL logging in ShareGPT format (already capturing training examples of when/why to use SESSION vs COMMAND)
- Fast blocking HTTP client talking to your local llama.cpp servers

### Current Status – In Active Testing
- COMMAND method is stable and reliable
- SESSION method works well for simple commands and basic persistence
- Output capture + summarizer flow is functional (double-summarization occurs but produces usable high-signal results for noisy commands)
- Deny list is active
- Logging is working and generating clean training data
- For build details go to [Doc/progress_log.md]()

**Not yet production-ready.** Persistent sessions with complex tools (full msfconsole workflows) are still being tuned. Context management and summarizer behavior continue to be refined. Database integration for auditing still to come.

### Quick Start

 1. Make sure your [llama.cpp](https://github.com/ggml-org/llama.cpp) servers are running
```bash
    - git clone https://github.com/ggml-org/llama.cpp
    - cd llama.cpp
    - cmake -B build
    - cmake --build build --config Release -j$(nproc)
```
    - Main model: port 8080
    - Summarizer (small model): port 8082
 2. Install dependencies
```bash
    - sudo apt install tmux
    - sudo apt install cargo
    - sudo apt install rustup
```
 3. **Build and run the Rust version**
```bash
  cd [build directory]
  cargo build --release
  ./target/release/echo_rust_wrapper
  ```
