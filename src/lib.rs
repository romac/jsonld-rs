#![feature(async_await)]
#![feature(await_macro)]
#![feature(const_vec_new)]
#![feature(futures_api)]
// #![feature(gen_future)]
// #![feature(generator_trait)]
// #![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate lazy_static;

extern crate url;

extern crate serde;
extern crate serde_json;

mod compact;
mod context;
mod creation;
mod expand;
pub mod nodemap;
pub mod rdf;

mod api;
pub use crate::api::*;

use std::error::Error;
use std::fmt::Debug;
use std::future::Future;

/// All the errors that may be returned by specific parts of the API.
pub mod error {
    pub use crate::compact::CompactionError;
    pub use crate::creation::{ContextCreationError, TermCreationError};
    pub use crate::expand::ExpansionError;
}

/// This trait is implemented by consumers of the API, to provide remote contexts.
pub trait RemoteContextLoader: Debug {
    type Error: Error + Send + Debug;
    type Future: Future<Output = Result<serde_json::Value, Self::Error>> + 'static;

    /// Loads a remote JSON-LD context into memory.
    fn load_context(url: String) -> Self::Future;
}
