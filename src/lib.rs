#![allow(clippy::collapsible_if)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::unnecessary_lazy_evaluations)]

pub mod cli;
pub mod commands;
pub mod condition;
pub mod config;
pub mod dashboard;
pub mod detect;
pub mod dispatch;
pub mod error;
pub mod logger;
pub mod model;
pub mod notify;
pub mod parser;
pub mod run_status;
pub mod runner;
pub mod summarize;
pub mod vars;
pub mod workspace;
