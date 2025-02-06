mod logfile;
mod chunk_reader;
mod drain;
mod logger;
mod logentry;
mod query_eval;
mod query_parsing;
mod log_buffer;
mod log_rotator;

pub use chunk_reader::ChunkReader;
pub use drain::{DrainParser, LogGroup, LogTemplate};
pub use logger::PuppylogBuilder;
pub use logentry::*;
pub use query_parsing::parse_log_query;
pub use query_parsing::QueryAst;
pub use query_eval::check_expr;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PuppylogEvent {
	QueryChanged {
		query: String
	}
}