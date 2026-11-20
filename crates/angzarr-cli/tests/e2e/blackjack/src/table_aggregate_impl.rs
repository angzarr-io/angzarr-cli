//! TableAggregateImpl: the table's logic is agg-table's `Table`, which
//! implements the table's interface over the domain layer the application
//! uses (examples-proto's): the generated adapter delegates to it, and
//! nothing is converted.

pub use agg_table::Table as TableAggregateImpl;
