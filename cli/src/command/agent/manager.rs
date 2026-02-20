//! Agent process manager.
//!
//! Handles spawning, monitoring, and terminating Claude Code child processes.
//! Communicates agent output and lifecycle events to the TUI event loop via
//! tokio channels.

use super::parser::Parser;
use super::state::{AgentEvent, DisplayLine};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};

/// Identifies whether a line originated from the child's stdout or stderr.
#[derive(Debug, Clone, Copy)]
enum LineSource {
    Stdout,
    Stderr,
}

/// Passthrough options forwarded to each spawned Claude Code process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClaudeOptions {
    /// Maps to `--permission-mode` (e.g. "acceptEdits", "plan", "bypassPermissions").
    pub permission_mode: Option<String>,
    /// Maps to `--allowedTools` (repeated). Note: camelCase on the Claude CLI side.
    pub allowed_tools: Vec<String>,
    /// Maps to `--model` (e.g. "sonnet", "opus").
    pub model: Option<String>,
    /// Maps to `--effort` (e.g. "low", "medium", "high").
    pub effort: Option<String>,
    /// Maps to `--max-budget-usd`.
    pub max_budget_usd: Option<f64>,
    /// Maps to `--add-dir` (repeated).
    pub add_dirs: Vec<String>,
}

/// Manages Claude Code child processes, spawning them and streaming their
/// output as [`AgentEvent`] values through a tokio channel.
pub struct AgentManager {
    /// Channel sender for dispatching events to the TUI event loop.
    event_tx: mpsc::Sender<AgentEvent>,
    /// Monotonically increasing ID assigned to the next spawned agent.
    next_id: usize,
    /// Kill signal senders keyed by agent ID. Sending on a channel tells the
    /// reader task to terminate the corresponding child process.
    kill_senders: HashMap<usize, oneshot::Sender<()>>,
    /// Tokio task handles for the reader/waiter tasks, keyed by agent ID.
    handles: HashMap<usize, JoinHandle<()>>,
    /// Child process PIDs keyed by agent ID, used for sending signals (e.g. SIGINT).
    child_pids: HashMap<usize, u32>,
}

impl AgentManager {
    /// Create a new manager that sends events through the given channel.
    pub fn new(event_tx: mpsc::Sender<AgentEvent>) -> Self {
        Self {
            event_tx,
            next_id: 0,
            kill_senders: HashMap::new(),
            handles: HashMap::new(),
            child_pids: HashMap::new(),
        }
    }

    /// Allocate the next agent ID without spawning a process.
    ///
    /// Used when creating agent tabs for loaded sessions that don't have
    /// a running Claude Code process.
    pub fn allocate_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Spawn a new Claude Code instance. Returns the assigned `agent_id`.
    ///
    /// The process is started with `--output-format stream-json --print --verbose`
    /// and its stdout/stderr are merged and parsed in a background tokio task.
    /// Events are sent through the channel as [`AgentEvent::Output`] and
    /// [`AgentEvent::Exited`].
    pub async fn spawn(
        &mut self,
        prompt: &str,
        workspace: &Path,
        claude_options: &ClaudeOptions,
        session_id: Option<&str>,
    ) -> Result<usize> {
        let agent_id = self.next_id;
        self.next_id += 1;

        info!(
            agent_id,
            workspace = %workspace.display(),
            prompt,
            ?session_id,
            "spawning claude agent"
        );

        let mut args = vec![
            "--include-partial-messages".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--print".to_string(),
            "--verbose".to_string(),
        ];

        if let Some(ref mode) = claude_options.permission_mode {
            args.push("--permission-mode".to_string());
            args.push(mode.clone());
        }

        for tool in &claude_options.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tool.clone());
        }

        if let Some(ref model) = claude_options.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }

        if let Some(ref effort) = claude_options.effort {
            args.push("--effort".to_string());
            args.push(effort.clone());
        }

        if let Some(budget) = claude_options.max_budget_usd {
            args.push("--max-budget-usd".to_string());
            args.push(budget.to_string());
        }

        for dir in &claude_options.add_dirs {
            args.push("--add-dir".to_string());
            args.push(dir.clone());
        }

        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }

        // Prompt must always be the last positional argument.
        args.push(prompt.to_string());

        let mut child = Command::new("claude")
            .args(&args)
            .current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("failed to spawn claude process: {e}. Is 'claude' (Claude Code CLI) installed and on PATH?"))?;

        // Store the child PID so we can send signals (e.g. SIGINT for interrupt).
        if let Some(pid) = child.id() {
            self.child_pids.insert(agent_id, pid);
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("failed to capture stderr"))?;

        let (kill_tx, mut kill_rx) = oneshot::channel::<()>();
        let tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            // Tag each stream with its source so stderr lines can be styled
            // differently from protocol errors on stdout.
            let stdout_lines =
                LinesStream::new(BufReader::new(stdout).lines()).map(|r| (LineSource::Stdout, r));
            let stderr_lines =
                LinesStream::new(BufReader::new(stderr).lines()).map(|r| (LineSource::Stderr, r));
            // NOTE: `tokio_stream::StreamExt::merge` polls the left stream
            // (stdout) before the right (stderr) on each iteration. Under heavy
            // stdout traffic — typical with stream-json protocol — stderr lines
            // may experience slight delivery delays. This is acceptable because
            // stderr only carries informational log output. If stderr latency
            // becomes a problem, consider `tokio::select!` over both streams
            // independently or a fair merge implementation.
            let mut merged = stdout_lines.merge(stderr_lines);
            let mut parser = Parser::new();

            loop {
                tokio::select! {
                    biased;

                    // Check for kill signal first (higher priority than reading).
                    _ = &mut kill_rx => {
                        info!(agent_id, "kill signal received, terminating agent process");
                        let _ = child.kill().await;
                        break;
                    }

                    line = merged.next() => {
                        match line {
                            Some((source, Ok(text))) => {
                                let display_lines = match source {
                                    // Stdout carries the stream-json protocol;
                                    // parse it normally.
                                    LineSource::Stdout => parser.parse_line(&text),
                                    // Stderr carries informational tracing/log
                                    // output — render it as dim text, not as
                                    // a parser error.
                                    LineSource::Stderr => {
                                        let trimmed = text.trim();
                                        if trimmed.is_empty() {
                                            Vec::new()
                                        } else {
                                            vec![DisplayLine::Stderr(text)]
                                        }
                                    }
                                };
                                // If the parser saw a session_id in a result
                                // event, emit it as a dedicated event so the
                                // TUI can store it on the agent state.
                                if let Some(session_id) = parser.take_session_id() {
                                    let _ = tx.send(AgentEvent::SessionId {
                                        agent_id,
                                        session_id,
                                    }).await;
                                }
                                if let Some(usage) = parser.take_usage() {
                                    let _ = tx.send(AgentEvent::UsageUpdate {
                                        agent_id,
                                        input_tokens: usage.input_tokens,
                                        output_tokens: usage.output_tokens,
                                        total_cost_usd: parser.take_cost(),
                                    }).await;
                                }
                                for display_line in display_lines {
                                    debug!(agent_id, ?display_line, "parsed output line");
                                    let _ = tx.send(AgentEvent::Output {
                                        agent_id,
                                        line: display_line,
                                    }).await;
                                }
                            }
                            Some((_source, Err(e))) => {
                                warn!(agent_id, error = %e, "stream read error");
                                let _ = tx.send(AgentEvent::Output {
                                    agent_id,
                                    line: DisplayLine::Error(format!("Read error: {e}")),
                                }).await;
                            }
                            None => {
                                debug!(agent_id, "output streams closed");
                                break;
                            }
                        }
                    }
                }
            }

            // Wait for the process to fully exit and retrieve the exit code.
            let exit_code = child.wait().await.ok().and_then(|s| s.code());

            info!(agent_id, ?exit_code, "agent process exited");

            let _ = tx
                .send(AgentEvent::Exited {
                    agent_id,
                    exit_code,
                })
                .await;
        });

        self.kill_senders.insert(agent_id, kill_tx);
        self.handles.insert(agent_id, handle);

        Ok(agent_id)
    }

    /// Kill a specific agent by ID.
    ///
    /// Sends a kill signal to the agent's background task, which terminates the
    /// child process. The reader task will send [`AgentEvent::Exited`] once
    /// the process has fully exited.
    ///
    /// # Borrow requirements
    ///
    /// This method takes `&mut self` because it removes the one-shot kill sender
    /// from the internal `kill_senders` map via [`HashMap::remove`]. A
    /// [`oneshot::Sender`] is consumed on send, so removal is the correct
    /// semantic — each agent can only be killed once.
    ///
    /// If concurrent kill support is ever needed (e.g. from multiple tokio
    /// tasks), the `kill_senders` field could be wrapped in a `Mutex` or
    /// replaced with a `DashMap` to allow `&self` access. For now the single-
    /// threaded TUI event loop is the only caller, so `&mut self` is fine.
    pub async fn kill(&mut self, agent_id: usize) -> Result<()> {
        info!(agent_id, "sending kill signal to agent");

        let kill_tx = self
            .kill_senders
            .remove(&agent_id)
            .ok_or_else(|| anyhow!("no agent with id {agent_id}"))?;

        // Send the kill signal. If the receiver is already gone (task finished),
        // this is harmless.
        let _ = kill_tx.send(());

        Ok(())
    }

    /// Interrupt a specific agent by sending SIGINT to the child process.
    ///
    /// Unlike [`kill`], which terminates the process immediately via the
    /// background task, this sends a SIGINT signal directly to the child
    /// process. Claude Code handles SIGINT gracefully — it stops the current
    /// generation and exits, preserving the session for later `--resume`.
    ///
    /// The agent is marked as interrupted so the TUI can distinguish an
    /// interrupt from a normal failure when the exit event arrives.
    pub async fn interrupt(&mut self, agent_id: usize) -> Result<()> {
        info!(agent_id, "sending SIGINT to agent");

        let pid = self
            .child_pids
            .get(&agent_id)
            .ok_or_else(|| anyhow!("no child process for agent {agent_id}"))?;

        // Use the POSIX kill command to send SIGINT to the child process.
        let status = Command::new("kill")
            .args(["-s", "INT", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| anyhow!("failed to run kill command: {e}"))?;

        if !status.success() {
            warn!(agent_id, pid, "kill -INT command failed");
            return Err(anyhow!("failed to send SIGINT to agent {agent_id}"));
        }

        Ok(())
    }

    /// Eagerly remove the stored child PID for an agent.
    ///
    /// Called as the very first step when an exit event is received, before
    /// any other handler logic runs. This prevents [`interrupt`] from sending
    /// SIGINT to a recycled PID if the OS reuses the PID between the exit
    /// event and the full [`notify_exited`] cleanup.
    pub fn remove_child_pid(&mut self, agent_id: usize) {
        self.child_pids.remove(&agent_id);
    }

    /// Clean up internal state for an agent that has exited.
    ///
    /// Should be called when an [`AgentEvent::Exited`] event is received so
    /// that the finished task handle is removed from the map, preventing
    /// unbounded growth of `handles`.
    pub fn notify_exited(&mut self, agent_id: usize) {
        self.kill_senders.remove(&agent_id);
        self.handles.remove(&agent_id);
        self.child_pids.remove(&agent_id);
    }

    /// Kill all agents and wait for all background tasks to complete.
    pub async fn kill_all(&mut self) {
        let count = self.kill_senders.len();
        info!(count, "killing all agents");

        // Send kill signals to all live agents.
        for (_id, kill_tx) in self.kill_senders.drain() {
            let _ = kill_tx.send(());
        }

        // Wait for all reader tasks to finish.
        for (_id, handle) in self.handles.drain() {
            if let Err(e) = handle.await {
                warn!(error = %e, "agent task panicked during shutdown");
            }
        }

        info!("all agent tasks completed");
    }
}
