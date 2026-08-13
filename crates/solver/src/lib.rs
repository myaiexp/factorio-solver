// Production-chain calculator and block generator.
//
// Re-exports the template library it builds on.
pub use factorio_templates;

pub mod availability;
pub mod chain;
pub mod layout;
pub mod recipe;

#[cfg(any(test, feature = "testsupport"))]
pub mod testsupport;
