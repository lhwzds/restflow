//! MCP (Model Context Protocol) server implementation for RestFlow
//!
//! This module implements an MCP server that allows Claude Code and other
//! MCP-compatible AI assistants to interact with RestFlow's skills, agents,
//! and workflows.

pub mod server;
pub mod tools;

pub use server::RestFlowMcpServer;
