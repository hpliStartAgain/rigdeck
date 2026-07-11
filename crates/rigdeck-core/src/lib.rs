//! RigDeck Core — Domain model, planner, and transaction engine.
//!
//! This crate contains the canonical domain model (Asset, AssetRevision,
//! Source, AgentAdapter, Assignment, DeploymentPlan, etc.) and the transactional
//! planner that generates, validates, and applies deployment plans.
//!
//! Core contains no Agent-name conditional branches. All agent-specific
//! behavior is expressed through the adapter contract defined in
//! `rigdeck-adapter-sdk`.

#![warn(missing_docs)]
