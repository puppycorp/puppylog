mod logfile;
mod chunk_reader;
mod drain;
mod logger;
mod logentry;
mod query_eval;
mod query_parsing;

pub use chunk_reader::ChunkReader;
pub use drain::{DrainParser, LogGroup, LogTemplate};
pub use logger::PuppylogBuilder;
pub use logentry::*;
pub use query_parsing::*;
pub use query_eval::check_expr;
pub use query_eval::check_props;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PuppylogEvent {
	QueryChanged {
		query: String
	}
}