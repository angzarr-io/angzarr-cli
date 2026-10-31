//! TableAggregate: the table's interface is agg-table's, scaffolded once
//! by `angzarr scaffold rust` over the same domain layer (examples-proto's),
//! which the generated adapter calls on agg-table's `Table`.

pub use agg_table::table_aggregate::TableAggregate;
