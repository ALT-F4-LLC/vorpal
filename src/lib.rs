pub mod api {
    tonic::include_proto!("vorpal.package.v0");
}
pub mod builder;
pub mod command;
pub mod database;
pub mod notary;
pub mod store;
extern crate tera;
