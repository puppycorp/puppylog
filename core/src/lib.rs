mod chunk_reader;
mod drain;
mod logentry;
mod logfile;
mod logger;
mod query_eval;
mod query_parsing;

pub use chunk_reader::ChunkReader;
pub use drain::{DrainParser, LogGroup, LogTemplate};
pub use logentry::*;
pub use logger::PuppylogBuilder;
pub use query_eval::check_expr;
pub use query_eval::check_props;
pub use query_eval::extract_date_conditions;
pub use query_eval::match_date_range;
pub use query_eval::timestamp_bounds;
pub use query_parsing::*;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PuppylogEvent {
	QueryChanged { query: String },
}
