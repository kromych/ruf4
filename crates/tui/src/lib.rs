// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//
// Portions derived from the Microsoft Edit project (https://github.com/microsoft/edit).
// See LICENSE-EDIT for the original MIT license.

#![allow(
    clippy::missing_transmute_annotations,
    clippy::missing_safety_doc,
    clippy::new_without_default,
    dead_code,
    stable_features,
    unexpected_cfgs
)]

pub mod buffer;
pub mod cell;
pub mod clipboard;
pub mod document;
pub mod framebuffer;
pub mod hash;
pub mod helpers;
pub mod input;
pub mod oklab;
pub mod simd;
pub mod sys;
pub mod tui;
pub mod unicode;
pub mod vt;
