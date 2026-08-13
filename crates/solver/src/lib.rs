// Production-chain calculator and block generator.
//
// Re-exports the template library it builds on.
pub use factorio_templates;

pub mod chain;
pub mod layout;
pub mod recipe;
pub mod tech;

#[cfg(any(test, feature = "testsupport"))]
pub mod testsupport;
